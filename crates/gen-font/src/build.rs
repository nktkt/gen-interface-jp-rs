//! Build pipeline orchestration.
//!
//! Rust port of `build_one`, `_get_variable_palt`, and the filesystem
//! constants from `source/src/font/build.py` (lines 85-810).
//!
//! Per-weight pipeline (mirrors the Python):
//! 1. **Bake** Noto Sans JP variable → static TTF at the target wght axis
//!    location, via [`crate::baker::bake`] with `MetadataMode::InheritBase`
//!    so Noto's name / OS2 records survive.
//! 2. **Proportionalise** the inst — read palt from the variable cache,
//!    bake into hmtx, apply tracking, apply per-glyph sidebearing tweaks,
//!    strip extreme-bbox glyphs, optionally apply x-scale.
//! 3. **Merge** the proportional Noto with the matching Inter master via
//!    [`crate::baker::merge_fonts`], with `SUB_EXCLUDE_CODEPOINTS` keeping
//!    CJK-conventional symbols on the Noto outline.
//!
//! NOTE: Stage 1 (`bake`) and Stage 3 (`merge_fonts`) currently bail with
//! TODO(impl) errors — the `skrifa`/`write-fonts` 0.42 surface for axis
//! pinning and font merge has not been wired up. Stage 2 is wired through
//! the implemented primitives (`palt::read_palt`,
//! `proportional::make_proportional`, `tracking::apply_tracking`,
//! `glyph_spacing::apply_glyph_spacing`, `strip_extreme::strip_extreme_glyphs`,
//! `x_scale::apply_x_scale`). End-to-end execution stops at Stage 1
//! regardless, so the Stage 2 wiring is unreachable today but compiles
//! and is ready to run once Stage 1 lands.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use skrifa::FontRef;
use write_fonts::FontBuilder;

use crate::baker::{
    self, AxisPin, ExportConfig, FontInput, MetadataMode, MetricsSource, OutputConfig,
};
use crate::classify;
use crate::families::{FamilyConfig, BASELINE_OFFSET, SCALE, SUB_EXCLUDE_CODEPOINTS};
use crate::proportional::ProportionalOptions;
use crate::weights::WeightSpec;

// ---------------------------------------------------------------------------
// BuildResult
// ---------------------------------------------------------------------------

/// Manifest of artefacts produced by a single [`build_one`] invocation.
#[derive(Debug, Clone)]
pub struct BuildResult {
    /// Absolute path to the merged final TTF written under `dist/ttf/<family>/`.
    pub font_path: PathBuf,
}

// ---------------------------------------------------------------------------
// BuildPaths
// ---------------------------------------------------------------------------

/// Filesystem layout for the build pipeline.
///
/// Mirrors the `ROOT` / `VENDOR_FONTS` / `INTER_DIR` / `NOTO_VARIABLE` /
/// `DIST` / `DIST_TTF` / `INTERMEDIATE` constants from
/// `source/src/font/build.py`. The Rust workspace deliberately does not
/// duplicate the Python project's `vendor/` directory — see `CLAUDE.md` —
/// so by default we read fonts from `../source/vendor/...` next door and
/// write outputs into `../source/dist/...`.
#[derive(Debug, Clone)]
pub struct BuildPaths {
    pub root: PathBuf,
    pub vendor_fonts: PathBuf,
    pub inter_dir: PathBuf,
    pub noto_variable: PathBuf,
    pub dist: PathBuf,
    pub dist_ttf: PathBuf,
    pub intermediate: PathBuf,
}

impl BuildPaths {
    /// Resolve build paths from a workspace root (the Python project root,
    /// at `../source/` relative to this Rust workspace).
    pub fn from_root(root: PathBuf) -> Self {
        let vendor_fonts = root.join("vendor").join("fonts");
        let inter_dir = vendor_fonts.join("Inter-4.1").join("extras").join("ttf");
        let noto_variable = vendor_fonts
            .join("Noto_Sans_JP")
            .join("NotoSansJP-VariableFont_wght.ttf");
        let dist = root.join("dist");
        let dist_ttf = dist.join("ttf");
        let intermediate = dist.join("intermediate");
        Self {
            root,
            vendor_fonts,
            inter_dir,
            noto_variable,
            dist,
            dist_ttf,
            intermediate,
        }
    }

    /// Default: assume the Rust workspace lives next to the original Python
    /// project (e.g. `../source/`) and reuse its `vendor/` directory.
    pub fn default_for_rust_workspace() -> Self {
        // Crate manifest dir is `<workspace>/crates/gen-font/`. The Python
        // source lives at `<workspace>/../source/`, so walk up three levels.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let root = Path::new(manifest_dir)
            .join("..") // crates/
            .join("..") // <workspace>/
            .join("..") // <repo>/
            .join("source");
        Self::from_root(root)
    }
}

// ---------------------------------------------------------------------------
// build_one
// ---------------------------------------------------------------------------

/// Build a single weight of a Gen Interface JP family.
///
/// Returns the path to the final merged TTF on success. Currently every stage
/// bails — see module-level note about the unwired font primitives.
pub fn build_one(
    paths: &BuildPaths,
    family: &FamilyConfig,
    weight: &WeightSpec,
) -> Result<BuildResult> {
    // ---- 0. Resolve paths and verify the Inter master exists ----
    let inter_path = paths.inter_dir.join(format!(
        "{}-{}.ttf",
        family.inter_prefix, weight.weight_name
    ));
    if !inter_path.is_file() {
        return Err(anyhow!("Inter font not found: {}", inter_path.display()));
    }

    std::fs::create_dir_all(&paths.intermediate)?;

    let inst_path = paths
        .intermediate
        .join(format!("NotoSansJP-{}-Inst.ttf", weight.weight_name));
    let prop_path = paths
        .intermediate
        .join(format!("NotoSansJP-{}-Prop.ttf", weight.weight_name));

    // ---- 1. Bake Noto variable -> static ----
    println!(
        "    [1/3] Baking Noto Sans JP (wght={})...",
        weight.noto_wght_axis
    );
    baker::bake(
        &FontInput {
            path: paths.noto_variable.clone(),
            scale: 1.0,
            baseline_offset: 0,
            axes: vec![AxisPin {
                tag: *b"wght",
                value: weight.noto_wght_axis as f32,
            }],
            exclude_codepoints: vec![],
        },
        &OutputConfig {
            weight: Some(weight.weight_num),
            metadata_mode: Some(MetadataMode::InheritBase),
            ..Default::default()
        },
        &ExportConfig {
            font_path: inst_path.clone(),
        },
    )?;

    // ---- 2. Proportionalise + tracking + glyph spacing + bbox strip ----
    let tracking = family.tracking;
    let tracking_kana = family.tracking_kana;
    let half_palt_punct = family.half_palt_punct;

    let mut desc = format!("tracking +{tracking}");
    if let Some(tk) = tracking_kana {
        desc.push_str(&format!(" (kana/punct +{tk})"));
    }
    if half_palt_punct {
        desc.push_str(" (punct half-palt)");
    }
    println!("    [2/3] Proportional (palt) + {desc}...");

    // Read palt adjustments from the variable Noto rather than the freshly
    // baked inst — variable→static instantiation can leave palt
    // ValueRecords with zeroed/shifted XPlacement/XAdvance pairs (see
    // `_get_variable_palt` in build.py:146-162). The variable font is the
    // canonical palt source across all weights.
    let variable_bytes = std::fs::read(&paths.noto_variable)
        .with_context(|| format!("read variable Noto: {}", paths.noto_variable.display()))?;
    let variable_font =
        FontRef::new(&variable_bytes).map_err(|e| anyhow!("parse variable Noto: {e}"))?;
    let palt_data = crate::palt::read_palt(&variable_font)?;

    // Stage 2 chains several mutations that all touch overlapping tables
    // (hmtx, glyf, hhea). Each primitive reads from `font` and writes to
    // `builder`; between primitives we therefore serialise + re-parse so
    // the next pass reads our mutations rather than the original inst
    // bytes. Mirrors the in-place mutation the Python `build_one` performs
    // on a single TTFont — this loop simulates that with a sequence of
    // owned `Vec<u8>` snapshots, since `skrifa::FontRef` borrows from a
    // `&[u8]` and can't be re-seated against a `FontBuilder` in place.
    let mut current_bytes: Vec<u8> = std::fs::read(&inst_path)
        .with_context(|| format!("read inst font: {}", inst_path.display()))?;

    // Pass A: proportional (palt → hmtx + glyf + GPOS feature strip).
    {
        let font = FontRef::new(&current_bytes)
            .map_err(|e| anyhow!("parse inst for proportional pass: {e}"))?;

        // Three-bucket policy active only when half_palt_punct is set.
        // Mirrors build.py:672-688.
        //
        // The classify helpers still surface glyph-name strings (a number of
        // them parse `uniXXXX`-style names by design); resolve through a
        // single `name → gid` map at the end so the gid-keyed
        // `ProportionalOptions` doesn't drag the name-based filtering down
        // into the primitive itself. Names that don't resolve in this font's
        // `post`/synthetic name table are silently dropped — same policy as
        // the Python reference's `if glyph_name in hmtx.metrics` filter.
        let (reduced_palt_set, squeeze_sb_set) = if half_palt_punct {
            // Build a name → gid resolver for this font. Names that the
            // `post` table doesn't expose come back as `gidNNN` placeholders
            // (skrifa fallback); the classify helpers below already return
            // names in the same vocabulary, so the lookup is a self-join.
            let glyph_names_resolver = skrifa::GlyphNames::new(&font);
            let mut name_to_gid: std::collections::BTreeMap<String, u32> =
                std::collections::BTreeMap::new();
            for (gid, name) in glyph_names_resolver.iter() {
                name_to_gid.insert(name.as_str().to_string(), gid.to_u32());
            }

            // palt_glyphs is keyed by gid (read_palt's new return shape);
            // cross-check against the classify name vocabulary by reverse-
            // resolving each gid to a name where possible, building a name
            // set for the filter passes below.
            let palt_gids: std::collections::BTreeSet<u32> = palt_data.keys().copied().collect();
            let palt_glyph_names: std::collections::BTreeSet<String> = palt_gids
                .iter()
                .filter_map(|&gid| {
                    glyph_names_resolver
                        .get(skrifa::GlyphId::new(gid))
                        .map(|n| n.to_string())
                })
                .collect();

            let vert_glyphs = classify::get_vert_alternates(&font)?;
            let cjk_glyphs = classify::get_cjk_glyphs(&font)?;
            let exclude: std::collections::BTreeSet<String> =
                vert_glyphs.union(&cjk_glyphs).cloned().collect();

            let reduced_palt_names: std::collections::BTreeSet<String> = palt_glyph_names
                .iter()
                .filter(|g| !classify::is_kana_letter(g) && !exclude.contains(*g))
                .cloned()
                .collect();

            // squeeze_sb: every glyph in the font's glyph order that is not
            // in palt, not in exclude, and not a kana letter.
            let glyph_order = classify::glyph_names(&font);
            let squeeze_sb_names: std::collections::BTreeSet<String> = glyph_order
                .into_iter()
                .filter(|g| {
                    !palt_glyph_names.contains(g)
                        && !exclude.contains(g)
                        && !classify::is_kana_letter(g)
                })
                .collect();

            // Resolve both name-sets to gid-sets, dropping unresolvable names
            // (same as Python's `if name in hmtx.metrics`).
            let reduced_palt: std::collections::BTreeSet<u32> = reduced_palt_names
                .iter()
                .filter_map(|n| name_to_gid.get(n).copied())
                .collect();
            let squeeze_sb: std::collections::BTreeSet<u32> = squeeze_sb_names
                .iter()
                .filter_map(|n| name_to_gid.get(n).copied())
                .collect();

            (Some(reduced_palt), Some(squeeze_sb))
        } else {
            (None, None)
        };

        let opts = ProportionalOptions {
            reduced_palt: reduced_palt_set,
            squeeze_sb: squeeze_sb_set,
            palt_override: Some(palt_data.clone()),
            ..Default::default()
        };

        let mut builder = FontBuilder::new();
        builder.copy_missing_tables(font.clone());
        crate::proportional::make_proportional(&font, &mut builder, &opts)?;
        current_bytes = builder.build();
    }

    // Pass B: tracking. Re-parse the post-proportional bytes and re-seed
    // the builder so tracking's hmtx rewrite reads the proportional
    // metrics, not the inst's untouched ones.
    {
        let font = FontRef::new(&current_bytes)
            .map_err(|e| anyhow!("parse post-proportional font for tracking pass: {e}"))?;
        let mut builder = FontBuilder::new();
        builder.copy_missing_tables(font.clone());
        crate::tracking::apply_tracking(&font, &mut builder, tracking, tracking_kana)?;
        current_bytes = builder.build();
    }

    // Pass C: per-glyph sidebearing tweaks (apply_glyph_spacing). Empty
    // spacing short-circuits inside the primitive; no harm in always
    // calling.
    {
        let font = FontRef::new(&current_bytes)
            .map_err(|e| anyhow!("parse post-tracking font for glyph_spacing pass: {e}"))?;
        let mut builder = FontBuilder::new();
        builder.copy_missing_tables(font.clone());
        let adjusted =
            crate::glyph_spacing::apply_glyph_spacing(&font, &mut builder, family.glyph_spacing)?;
        if adjusted > 0 {
            println!("          Per-glyph spacing: {adjusted} glyph(s) adjusted");
        }
        current_bytes = builder.build();
    }

    // Pass D: strip extreme-bbox glyphs (vertical-only iteration marks etc.).
    {
        let font = FontRef::new(&current_bytes)
            .map_err(|e| anyhow!("parse post-spacing font for strip_extreme pass: {e}"))?;
        let mut builder = FontBuilder::new();
        builder.copy_missing_tables(font.clone());
        crate::strip_extreme::strip_extreme_glyphs(&font, &mut builder)?;
        current_bytes = builder.build();
    }

    // Pass E (optional): horizontal x-scale. Skipped when scale == 1.0
    // (the current FAMILIES default).
    if (family.x_scale - 1.0).abs() > f32::EPSILON {
        let font = FontRef::new(&current_bytes)
            .map_err(|e| anyhow!("parse post-strip font for x_scale pass: {e}"))?;
        let mut builder = FontBuilder::new();
        builder.copy_missing_tables(font.clone());
        crate::x_scale::apply_x_scale(&font, &mut builder, family.x_scale)?;
        current_bytes = builder.build();
    }

    std::fs::write(&prop_path, &current_bytes)
        .with_context(|| format!("write prop font: {}", prop_path.display()))?;

    {
        let family_name = family.family_name;
        let file_name = format!("{}-{}", family.folder_prefix, weight.weight_name);
        let ttf_dir = paths.dist_ttf.join(family_name);
        let out_path = ttf_dir.join(format!("{file_name}.ttf"));
        println!(
            "    [3/3] Merging {} + proportional Noto...",
            family.inter_prefix
        );
        baker::merge_fonts(
            &FontInput {
                path: inter_path,
                scale: 1.0,
                baseline_offset: 0,
                axes: vec![],
                exclude_codepoints: SUB_EXCLUDE_CODEPOINTS
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
            },
            &FontInput {
                path: prop_path,
                scale: SCALE,
                baseline_offset: BASELINE_OFFSET,
                axes: vec![],
                exclude_codepoints: vec![],
            },
            &OutputConfig {
                family_name: Some(family_name.to_string()),
                weight: Some(weight.weight_num),
                italic: Some(false),
                width: Some(5),
                metrics_source: Some(MetricsSource::Sub),
                manufacturer: Some("Yamato Iizuka".to_string()),
                manufacturer_url: Some("https://yamatoiizuka.com".to_string()),
                ..Default::default()
            },
            &ExportConfig {
                font_path: out_path.clone(),
            },
        )?;
        Ok(BuildResult {
            font_path: out_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_paths_from_root_layout() {
        let paths = BuildPaths::from_root(PathBuf::from("/x/source"));
        assert_eq!(paths.root, PathBuf::from("/x/source"));
        assert_eq!(paths.vendor_fonts, PathBuf::from("/x/source/vendor/fonts"));
        assert_eq!(
            paths.inter_dir,
            PathBuf::from("/x/source/vendor/fonts/Inter-4.1/extras/ttf")
        );
        assert_eq!(
            paths.noto_variable,
            PathBuf::from("/x/source/vendor/fonts/Noto_Sans_JP/NotoSansJP-VariableFont_wght.ttf")
        );
        assert_eq!(paths.dist, PathBuf::from("/x/source/dist"));
        assert_eq!(paths.dist_ttf, PathBuf::from("/x/source/dist/ttf"));
        assert_eq!(
            paths.intermediate,
            PathBuf::from("/x/source/dist/intermediate")
        );
    }
}
