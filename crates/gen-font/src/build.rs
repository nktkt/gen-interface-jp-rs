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
//! NOTE: Stage 1 (`bake`), Stage 2 (in-place font mutation), and Stage 3
//! (`merge_fonts`) all currently bail with TODO(impl) errors — the
//! `skrifa`/`write-fonts` 0.42 surface for axis pinning, hmtx/glyf
//! mutation, and font merge has not been wired up. The orchestration
//! below shows the intended call sequence so wiring is mechanical once
//! those primitives land.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};

use crate::baker::{
    self, AxisPin, ExportConfig, FontInput, MetadataMode, MetricsSource, OutputConfig,
};
use crate::families::{FamilyConfig, BASELINE_OFFSET, SCALE, SUB_EXCLUDE_CODEPOINTS};
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
    let inter_path = paths
        .inter_dir
        .join(format!("{}-{}.ttf", family.inter_prefix, weight.weight_name));
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

    // TODO(impl): open inst_path with skrifa, build a write-fonts FontBuilder,
    // then run:
    //   crate::proportional::make_proportional(&mut builder, opts)?;
    //   crate::tracking::apply_tracking(&mut builder, tracking, tracking_kana)?;
    //   crate::glyph_spacing::apply_glyph_spacing(&mut builder, family.glyph_spacing)?;
    //   crate::strip_extreme::strip_extreme_glyphs(&mut builder)?;
    //   if family.x_scale != 1.0 { crate::x_scale::apply_x_scale(&mut builder, family.x_scale)?; }
    //   std::fs::write(&prop_path, builder.build()?)?;
    let _ = (&prop_path, SUB_EXCLUDE_CODEPOINTS, BASELINE_OFFSET, SCALE);

    bail!(
        "build_one: Stage 2 (proportionalise) requires write-fonts 0.42 \
         hmtx/glyf mutation that is not yet wired up — see TODO(impl) in \
         gen_font::proportional / gen_font::tracking / gen_font::glyph_spacing"
    );

    // The remaining code is intentionally unreachable but documents the
    // intended Stage 3 wiring once Stage 2 lands.
    #[allow(unreachable_code)]
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
                exclude_codepoints: SUB_EXCLUDE_CODEPOINTS.iter().map(|s| s.to_string()).collect(),
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
        Ok(BuildResult { font_path: out_path })
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
            PathBuf::from(
                "/x/source/vendor/fonts/Noto_Sans_JP/NotoSansJP-VariableFont_wght.ttf"
            )
        );
        assert_eq!(paths.dist, PathBuf::from("/x/source/dist"));
        assert_eq!(paths.dist_ttf, PathBuf::from("/x/source/dist/ttf"));
        assert_eq!(
            paths.intermediate,
            PathBuf::from("/x/source/dist/intermediate")
        );
    }
}
