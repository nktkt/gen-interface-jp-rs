//! Build tiny font subsets for the site Composition section's build-time
//! SVG shape generation.
//!
//! Port of `../source/src/font/site_subset.py`. Two passes:
//!
//! - **Noto Sans JP Variable** -> only the characters `"書体デザイン"`, keeping
//!   the wght axis and the `palt`+`kern` features (the JP shape data tweens
//!   on `palt`-adjusted positions).
//! - **Gen Interface JP Regular** -> only `"Type Design"`, keeping `kern` so
//!   the Latin reference renders through the same SVG path with matching
//!   pair adjustments.
//!
//! Both subsets keep `glyph_names = true` and `name_IDs = [1, 2, 3, 4, 6]`
//! and drop no tables.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Subset configuration shared across the two passes.
///
/// Mirrors the `fontTools.subset.Options` knobs that the Python script sets,
/// so the inline subsetter below has a single place to read its policy from.
//
// The fields are documenting intended config knobs for the unimplemented
// subsetter; until `build_subset` is wired up they are read only via the
// `let _ = cfg;` anchor below, hence the blanket `dead_code` allow.
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct SubsetConfig<'a> {
    /// Characters whose glyphs (and their dependencies) must survive.
    chars: &'a str,
    /// OpenType layout features to retain (e.g. `["palt", "kern"]`).
    layout_features: &'a [&'a str],
    /// Whether to keep `post` glyph names.
    glyph_names: bool,
    /// `name` table records to keep (matches the Python `name_IDs` list).
    name_ids: &'a [u16],
    /// Tables to drop unconditionally. Empty = drop none.
    drop_tables: &'a [&'a str],
}

impl<'a> SubsetConfig<'a> {
    fn new(chars: &'a str, layout_features: &'a [&'a str]) -> Self {
        Self {
            chars,
            layout_features,
            glyph_names: true,
            // Family / Subfamily / Unique ID / Full name / PostScript name.
            name_ids: &[1, 2, 3, 4, 6],
            drop_tables: &[],
        }
    }
}

/// Build the two site-subset TTFs into `out_dir`.
///
/// `noto_src` and `gen_src` are the source TTF paths. Returns the paths of
/// the two output files in the same order: `(noto_out, gen_out)`.
pub fn build_site_subsets(
    noto_src: &Path,
    gen_src: &Path,
    out_dir: &Path,
) -> Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("creating site-subset out dir {}", out_dir.display()))?;

    let noto_out = out_dir.join("NotoSansJP-Subset.ttf");
    let gen_out = out_dir.join("GenInterfaceJP-Regular-Subset.ttf");

    // palt is what the generated JP shape data stores for tweening.
    build_subset(
        noto_src,
        &noto_out,
        &SubsetConfig::new("書体デザイン", &["palt", "kern"]),
    )?;
    // Latin side just needs basic shaping (kern for Inter's pair adjustments).
    build_subset(
        gen_src,
        &gen_out,
        &SubsetConfig::new("Type Design", &["kern"]),
    )?;

    Ok((noto_out, gen_out))
}

/// Run a single subset pass: load `src`, keep only the glyphs reachable from
/// `cfg.chars`, and write the result to `dst`.
///
/// Implementation outline (skrifa for read, write-fonts for emit):
///
/// 1. Load the source TTF and parse it with `skrifa::FontRef`.
/// 2. Walk the `cmap` to map each codepoint in `cfg.chars` to a glyph id;
///    union those with glyph 0 (`.notdef`) and any composite/`GSUB`-reachable
///    dependencies.
/// 3. Build a `write_fonts::FontBuilder`, copying through the tables we keep
///    (`head`, `hhea`, `hmtx`, `maxp`, `name`, `OS/2`, `post`, `cmap`,
///    `glyf`+`loca` or `CFF`, `GDEF`, `GSUB`, `GPOS`, `fvar`, `gvar`, ...)
///    with each table rewritten to reference only the surviving glyphs and
///    the layout features listed in `cfg.layout_features`.
/// 4. Serialise and write to `dst`.
///
/// The skrifa/write-fonts surface for steps 2-3 is wide; the exact entry
/// points are marked `TODO(api):` below.
#[allow(unused_variables)]
fn build_subset(src: &Path, dst: &Path, cfg: &SubsetConfig<'_>) -> Result<()> {
    let bytes =
        std::fs::read(src).with_context(|| format!("reading source font {}", src.display()))?;

    // TODO(api): replace this placeholder with the real subsetter.
    //
    // The Python reference uses `fontTools.subset.Subsetter`; the Rust port
    // needs the equivalent built on `skrifa` + `write-fonts`. The shape we
    // want is roughly:
    //
    //     let font = skrifa::FontRef::new(&bytes)?;
    //     let gids = collect_gids_for_text(&font, cfg.chars)?;
    //     let subset = SubsetBuilder::new(&font)
    //         .keep_glyphs(&gids)
    //         .keep_layout_features(cfg.layout_features)
    //         .keep_name_ids(cfg.name_ids)
    //         .glyph_names(cfg.glyph_names)
    //         .drop_tables(cfg.drop_tables)
    //         .build()?;
    //     std::fs::write(dst, subset)?;
    //
    // Until that subsetter lives in this crate (or `gen-webfont` exposes one
    // we can depend on without a cycle), pass-through the source bytes so
    // the public API is callable end-to-end. The output is *not* a real
    // subset yet -- this is a structural placeholder.
    let _ = cfg; // silence unused-field warnings while the body is a stub.
    std::fs::write(dst, &bytes).with_context(|| format!("writing subset {}", dst.display()))?;

    let size_kb = std::fs::metadata(dst)
        .with_context(|| format!("stat {}", dst.display()))?
        .len() as f64
        / 1024.0;
    println!("Wrote {} ({:.1} KB)", dst.display(), size_kb);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time check that the public signature matches the spec.
    #[allow(dead_code)]
    fn _check_signature() {
        let _f: fn(&Path, &Path, &Path) -> Result<(PathBuf, PathBuf)> = build_site_subsets;
    }
}
