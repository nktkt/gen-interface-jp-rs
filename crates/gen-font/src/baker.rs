//! In-tree port of the Python `ofl-font-baker` library.
//!
//! `ofl-font-baker` exists as a separate package on the Python side; no Rust
//! equivalent is published. Rather than vend a thin wrapper around a foreign
//! tool, the build pipeline owns these primitives directly so we can iterate
//! on them without an extra release boundary. The two operations the rest of
//! the pipeline depends on are:
//!
//! 1. **`bake`** — pin the axes of a variable font (e.g. `wght=465`) and emit
//!    a static TTF. This is the "Stage 1" entry point: variable Noto in,
//!    static Noto out at a fixed weight.
//!
//! 2. **`merge_fonts`** — composite a sub font on top of a base font by
//!    codepoint (sub overrides base, except for `excludeCodepoints`). This is
//!    "Stage 3": a proportionalised JP font merged with Inter into a single
//!    final family.
//!
//! ### Why `MetadataMode::InheritBase` is the default we care about
//!
//! When we instance Noto at a fixed weight we do *not* want to mint a new
//! identity for the resulting font — we want the static TTF to keep Noto's
//! `name`, `OS/2`, and `post` records intact: designer attribution, the OFL
//! license text, the manufacturer string, the version stamp. Stripping or
//! rewriting those would both misrepresent provenance and break OFL
//! compliance. `InheritBase` therefore preserves the upstream font's identity
//! exactly across instantiation; only the metrics-affecting tables change.
//!
//! `InheritSub` exists for the merge case where the sub font (Noto JP) is the
//! "primary" identity holder. `Override` is the escape hatch for tests and
//! for the rare case where the build config needs to stamp a new family name
//! end-to-end.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

/// Controls which font's `name`/`OS/2`/`post` records survive into the
/// output. See module docs for the rationale behind `InheritBase`.
#[derive(Debug, Clone)]
pub enum MetadataMode {
    /// Keep the base font's name/OS2/post records intact.
    ///
    /// Used when instancing Noto: we want Noto's designer credit, OFL block,
    /// manufacturer, and version string to survive the bake unchanged.
    InheritBase,
    /// Use sub font's records.
    ///
    /// Used when the sub font is the identity-bearing one in a merge
    /// (e.g. JP-primary families).
    InheritSub,
    /// Stamp from output config only.
    ///
    /// Escape hatch — every name record is rewritten from `OutputConfig`.
    Override,
}

/// Which input contributes the vertical metrics (`hhea`, `OS/2` ascent/descent
/// fields, `vhea` if present) to the merged output. The other input's
/// vertical metrics are discarded; sub and base typically disagree so a
/// choice has to be made explicit at config time.
#[derive(Debug, Clone, Copy)]
pub enum MetricsSource {
    Sub,
    Base,
}

/// A single axis pin like `wght=465` or `wdth=100`.
#[derive(Debug, Clone)]
pub struct AxisPin {
    pub tag: [u8; 4],
    pub value: f32,
}

/// One input font to the baker. `scale` and `baseline_offset` apply only
/// during merge and are typically set on the sub side to match the base
/// font's cap/x-heights and baseline.
#[derive(Debug, Clone)]
pub struct FontInput {
    pub path: PathBuf,
    pub scale: f32,
    pub baseline_offset: i32,
    pub axes: Vec<AxisPin>,
    /// Codepoints to strip from this font's cmap before merge. Used to
    /// remove glyphs the sub font carries by convention but the base font
    /// owns more authoritatively (CJK-conventional Latin/symbol glyphs that
    /// would otherwise shadow the base).
    pub exclude_codepoints: Vec<String>,
}

/// Output identity and policy. All fields are optional; `None` means "fall
/// back to whatever `metadata_mode` decided".
#[derive(Debug, Clone, Default)]
pub struct OutputConfig {
    pub family_name: Option<String>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub width: Option<u8>,
    pub metrics_source: Option<MetricsSource>,
    pub metadata_mode: Option<MetadataMode>,
    pub manufacturer: Option<String>,
    pub manufacturer_url: Option<String>,
}

/// Where to write the result.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub font_path: PathBuf,
}

/// Bake a variable font to a static TTF. Single-input — base only.
///
/// Steps:
/// 1. Read the variable font with `skrifa`.
/// 2. Pin every axis listed in `base.axes` via the variation instancer.
/// 3. Apply `OutputConfig` according to `metadata_mode` (defaulting to
///    `InheritBase` so Noto's identity records survive — see module docs).
/// 4. Serialise via `write-fonts` to `export.font_path`.
pub fn bake(base: &FontInput, output: &OutputConfig, export: &ExportConfig) -> Result<()> {
    // TODO(impl): wire skrifa::FontRef + variation instancer here. The
    // instancer API is in flux upstream; pin the version in Cargo.toml
    // before fleshing this out so we can lean on a stable surface.
    //
    // Sketch:
    //   let bytes = std::fs::read(&base.path)?;
    //   let font = skrifa::FontRef::new(&bytes)?;
    //   let instanced = instancer::pin_axes(&font, &base.axes)?;
    //   let mut builder = write_fonts::FontBuilder::new();
    //   copy_tables(&instanced, &mut builder, output, MetadataMode::InheritBase);
    //   std::fs::write(&export.font_path, builder.build()?)?;
    let _ = (base, output, export);
    Err(anyhow!(
        "bake: variable-font instancing not yet implemented; \
         see TODO(impl) in gen_font::baker"
    ))
}

/// Merge sub + base into a single composite font.
///
/// Steps:
/// 1. Read both fonts with `skrifa`.
/// 2. Expand `sub.exclude_codepoints` into a set of `u32` codepoints.
/// 3. Strip those entries from sub's cmap.
/// 4. Build the merged cmap: start from base, overlay sub (sub wins).
/// 5. Detect glyph-name collisions and rename sub-side entries
///    (`<name>` -> `<name>.sub`) so `post` table names stay unique.
/// 6. Apply `output.metrics_source` to choose vertical metrics.
/// 7. Apply `metadata_mode`/`output` to stamp identity (`familyName`,
///    `weight`, `manufacturer`, `manufacturerUrl`).
/// 8. Serialise via `write-fonts`.
pub fn merge_fonts(
    sub: &FontInput,
    base: &FontInput,
    output: &OutputConfig,
    export: &ExportConfig,
) -> Result<()> {
    // Eagerly parse exclusion specs so a bad config errors out before we
    // touch the filesystem.
    let _excluded: Vec<u32> = parse_codepoint_spec(&sub.exclude_codepoints)
        .context("failed to parse sub.exclude_codepoints")?;

    // TODO(impl): the merge pipeline. Rough sketch:
    //
    //   let sub_bytes = std::fs::read(&sub.path)?;
    //   let base_bytes = std::fs::read(&base.path)?;
    //   let sub_font = skrifa::FontRef::new(&sub_bytes)?;
    //   let base_font = skrifa::FontRef::new(&base_bytes)?;
    //
    //   let sub_cmap = read_cmap(&sub_font);
    //   let sub_cmap = strip_codepoints(sub_cmap, &excluded);
    //   let base_cmap = read_cmap(&base_font);
    //   let merged_cmap = overlay(base_cmap, sub_cmap); // sub wins
    //
    //   let renames = detect_collisions(&sub_font, &base_font);
    //   let sub_renamed = rename_glyphs(&sub_font, &renames);
    //
    //   let mut builder = write_fonts::FontBuilder::new();
    //   copy_glyphs(&base_font, &mut builder);
    //   copy_glyphs(&sub_renamed, &mut builder, scale=sub.scale,
    //               y_offset=sub.baseline_offset);
    //   write_cmap(&mut builder, &merged_cmap);
    //   write_metrics(&mut builder, output.metrics_source, &sub_font, &base_font);
    //   stamp_identity(&mut builder, output, metadata_mode);
    //   std::fs::write(&export.font_path, builder.build()?)?;
    let _ = (sub, base, output, export);
    Err(anyhow!(
        "merge_fonts: cmap merge / glyph rename / metrics blend \
         not yet implemented; see TODO(impl) in gen_font::baker"
    ))
}

/// Parse codepoint specs like `"U+25CE"` or `"U+2460-U+2469"` into a flat
/// list of codepoints (ranges expanded).
///
/// Accepts:
/// - `"U+XXXX"` — single codepoint.
/// - `"U+XXXX-U+YYYY"` — inclusive range. The `U+` prefix on the second
///   half is required to keep the syntax unambiguous.
///
/// Returns `Err` on any malformed entry rather than silently skipping —
/// silent skipping in font configs has bitten us before (a typo'd
/// codepoint in `excludeCodepoints` shipped an unintended glyph).
pub fn parse_codepoint_spec(specs: &[String]) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    for spec in specs {
        let spec = spec.trim();
        if let Some((lo, hi)) = split_range(spec) {
            let lo = parse_one(lo)
                .with_context(|| format!("invalid codepoint range start in {spec:?}"))?;
            let hi = parse_one(hi)
                .with_context(|| format!("invalid codepoint range end in {spec:?}"))?;
            if hi < lo {
                return Err(anyhow!(
                    "codepoint range {spec:?} has end < start ({hi:#X} < {lo:#X})"
                ));
            }
            for cp in lo..=hi {
                out.push(cp);
            }
        } else {
            out.push(
                parse_one(spec).with_context(|| format!("invalid codepoint spec {spec:?}"))?,
            );
        }
    }
    Ok(out)
}

/// Split `"U+2460-U+2469"` into `("U+2460", "U+2469")`.
///
/// We split on the *second* `U+` rather than the first `-` so that a single
/// `U+` prefix is unambiguous. (Hex codepoints can't contain `-` themselves,
/// but defending against it costs nothing.)
fn split_range(spec: &str) -> Option<(&str, &str)> {
    // The second occurrence of "U+" — anywhere after position 1 — marks
    // the start of the range's upper bound.
    let after_first = spec.get(2..)?;
    let second = after_first.find("U+")?;
    // `second` is an offset into `after_first`; rebase to `spec`.
    let split_at = 2 + second;
    let lo = spec.get(..split_at)?.trim_end_matches('-').trim_end();
    let hi = spec.get(split_at..)?;
    Some((lo, hi))
}

/// Parse a single `"U+XXXX"` (case-insensitive `U+`) into a `u32`.
fn parse_one(s: &str) -> Result<u32> {
    let s = s.trim();
    let hex = s
        .strip_prefix("U+")
        .or_else(|| s.strip_prefix("u+"))
        .ok_or_else(|| anyhow!("expected U+ prefix in {s:?}"))?;
    u32::from_str_radix(hex, 16)
        .map_err(|e| anyhow!("invalid hex in {s:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_codepoint() {
        let got = parse_codepoint_spec(&["U+25CE".into()]).unwrap();
        assert_eq!(got, vec![0x25CE]);
    }

    #[test]
    fn parse_range_codepoint() {
        let got = parse_codepoint_spec(&["U+2460-U+2469".into()]).unwrap();
        let want: Vec<u32> = (0x2460..=0x2469).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn parse_mixed_specs() {
        let got = parse_codepoint_spec(&[
            "U+0041".into(),
            "U+2460-U+2462".into(),
        ])
        .unwrap();
        assert_eq!(got, vec![0x0041, 0x2460, 0x2461, 0x2462]);
    }

    #[test]
    fn parse_bogus_errors() {
        let err = parse_codepoint_spec(&["bogus".into()]);
        assert!(err.is_err(), "expected parse error, got {err:?}");
    }

    #[test]
    fn parse_inverted_range_errors() {
        let err = parse_codepoint_spec(&["U+2469-U+2460".into()]);
        assert!(err.is_err(), "expected inverted-range error, got {err:?}");
    }

    #[test]
    fn parse_empty_is_empty() {
        let got = parse_codepoint_spec(&[]).unwrap();
        assert!(got.is_empty());
    }
}
