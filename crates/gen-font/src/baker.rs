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

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, MetadataProvider};
use write_fonts::{from_obj::ToOwnedTable, tables::os2::Os2, FontBuilder};

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
/// # Current implementation status — fallback passthrough
///
/// **This function is a fallback, not a real bake.** The current body:
///
/// 1. Reads `base.path` with `skrifa`.
/// 2. If `base.axes` is non-empty, emits a `eprintln!` warning that the axes
///    will *not* be pinned — the caller's pin list is observed only to log
///    it.
/// 3. If `output.weight` is set, copies the source `OS/2` table, overwrites
///    `us_weight_class` with that weight, and adds it to a fresh
///    `FontBuilder`. Other `OS/2` fields are preserved.
/// 4. Forwards every other table verbatim via `FontBuilder::copy_missing_tables`
///    (so glyf/gvar/fvar/avar/HVAR/MVAR/STAT/cmap/name/post/etc. all survive
///    untouched, including the variation machinery).
/// 5. Writes the rebuilt bytes to `export.font_path`. The output is a
///    structurally valid TTF (checksums recomputed, table directory rebuilt
///    by `FontBuilder::build`), but it is still a **variable font** carrying
///    the source's full default-master glyph set.
///
/// `output.italic`, `output.width`, `output.family_name`, `output.manufacturer*`,
/// and `output.metadata_mode` are **not consulted** in the current fallback.
/// `MetadataMode::InheritBase`-style behaviour falls out implicitly because
/// every identity table is forwarded verbatim, but the mode itself is not
/// branched on.
///
/// # TODO — real bake
///
/// - **fvar axis instancing.** Walk `base.axes`, look up each tag in `fvar`,
///   and resolve the pin to a normalised axis coordinate.
/// - **gvar / HVAR / MVAR delta application.** Apply per-glyph gvar deltas
///   to `glyf` outlines at the resolved coordinates; apply HVAR deltas to
///   horizontal advances; apply MVAR deltas to font-wide metrics
///   (`OS/2.sTypoAscender` etc., `hhea.ascender`, …).
/// - **Variation-table strip.** After deltas have been baked into the static
///   tables, drop `fvar`, `gvar`, `avar`, `HVAR`, `MVAR`, and `STAT` from
///   the output so the file is no longer advertised as variable.
/// - **Honour the rest of `OutputConfig`.** Stamp `output.italic` into
///   `OS/2.fsSelection` + `head.macStyle`, `output.width` into
///   `OS/2.usWidthClass`, and branch on `MetadataMode::Override` /
///   `InheritSub` instead of always behaving like `InheritBase`.
pub fn bake(base: &FontInput, output: &OutputConfig, export: &ExportConfig) -> Result<()> {
    let bytes = std::fs::read(&base.path)
        .with_context(|| format!("read base font: {}", base.path.display()))?;
    let font = FontRef::new(&bytes)
        .map_err(|e| anyhow!("parse base font {}: {e}", base.path.display()))?;

    if !base.axes.is_empty() {
        eprintln!(
            "warning: gen_font::baker::bake is currently a passthrough — \
             axes {:?} will NOT be pinned in the output. Downstream Stage 2 \
             will operate on the variable font's default master.",
            base.axes
                .iter()
                .map(|p| (std::str::from_utf8(&p.tag).unwrap_or("?"), p.value))
                .collect::<Vec<_>>()
        );
    }

    let mut builder = FontBuilder::new();

    // Stamp OS/2.usWeightClass from output.weight when set. Honours
    // `MetadataMode::InheritBase` (the build-pipeline default): every other
    // identity field on OS/2 is preserved from the source font, only the
    // weight class is overridden so downstream Stage 3 merge sees the right
    // weight bucket. See module docs for the rationale.
    //
    // We always stamp when output.weight is Some, regardless of metadata_mode
    // — Override and InheritSub haven't been wired up yet, so they fall
    // through to the same behaviour as InheritBase. When those modes land,
    // this branch needs to be gated.
    if let Some(weight) = output.weight {
        if let Ok(os2_src) = font.os2() {
            let mut os2: Os2 = os2_src.to_owned_table();
            os2.us_weight_class = weight;
            builder
                .add_table(&os2)
                .map_err(|e| anyhow!("stamp OS/2.usWeightClass={weight}: {e}"))?;
        }
        // If the source has no OS/2 (rare for any modern font), we silently
        // skip the stamp — copy_missing_tables will forward whatever the
        // source had.
    }

    // Forward every other table verbatim from the source. For tables we've
    // already added (OS/2 above), copy_missing_tables is a no-op.
    builder.copy_missing_tables(font);

    let out_bytes = builder.build();

    if let Some(parent) = export.font_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create output dir: {}", parent.display()))?;
        }
    }
    std::fs::write(&export.font_path, &out_bytes)
        .with_context(|| format!("write baked font: {}", export.font_path.display()))?;

    Ok(())
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
#[allow(unused_variables)]
pub fn merge_fonts(
    sub: &FontInput,
    base: &FontInput,
    output: &OutputConfig,
    export: &ExportConfig,
) -> Result<()> {
    // Eagerly parse exclusion specs so a bad config errors out before we
    // touch the filesystem.
    let excluded: BTreeSet<u32> = parse_codepoint_spec(&sub.exclude_codepoints)
        .context("failed to parse sub.exclude_codepoints")?
        .into_iter()
        .collect();

    // ---- Fallback policy (base bytes verbatim, with cmap diff log) -----
    //
    // The full merge is sketched in the docstring above (cmap union, glyph
    // copy with scale/baseline_offset, hmtx/hhea per metrics_source, name
    // table stamping, GPOS/GSUB rebuild) — a sizeable port of the Python
    // `ofl-font-baker` merge half. Rather than ship half a merge that
    // silently emits a broken font, we emit the **base font's bytes
    // verbatim** under `export.font_path` so the rest of the build
    // pipeline (Stage 3 onward, downstream WOFF2 / release crates) keeps
    // running end-to-end. The cmap diff that the real merge would compute
    // is logged so the overlay decisions stay observable while the
    // glyph-copy half is being fleshed out.
    //
    // TODO(impl): full merge pipeline.
    //   1. Renumber sub's glyphs to land after base's (gid_offset = N_base).
    //   2. Detect glyph-name collisions (post.names) and rename sub side
    //      `<name>` -> `<name>.sub` so post-table names stay unique.
    //   3. Build merged glyf+loca via GlyfLocaBuilder in new gid order,
    //      applying base.scale / base.baseline_offset to base outlines and
    //      sub.scale / sub.baseline_offset to sub outlines.
    //   4. Build merged hmtx; pick hhea per `output.metrics_source`.
    //      Advances/lsbs come from each glyph's source font, scaled.
    //   5. Build merged cmap from the union (base ∪ sub minus excluded),
    //      with sub overriding base on collisions.
    //   6. Stamp identity (name table) from `output` per `metadata_mode`.
    //   7. Pass GPOS/GSUB through (renumbered) or skip.
    let sub_bytes = std::fs::read(&sub.path)
        .with_context(|| format!("read sub font: {}", sub.path.display()))?;
    let base_bytes = std::fs::read(&base.path)
        .with_context(|| format!("read base font: {}", base.path.display()))?;

    let sub_font = FontRef::new(&sub_bytes).map_err(|e| anyhow!("parse sub font: {e}"))?;
    let base_font = FontRef::new(&base_bytes).map_err(|e| anyhow!("parse base font: {e}"))?;

    // Walk both charmaps. We don't actually rewrite cmap yet (see
    // TODO(impl) above) but we surface the diff so the overlay decisions
    // are observable while the glyph-copy half is still missing.
    let base_cps: BTreeSet<u32> = base_font.charmap().mappings().map(|(cp, _)| cp).collect();
    let sub_cps: BTreeSet<u32> = sub_font.charmap().mappings().map(|(cp, _)| cp).collect();

    let sub_only: usize = sub_cps
        .difference(&base_cps)
        .filter(|cp| !excluded.contains(cp))
        .count();
    let sub_overrides: usize = sub_cps
        .intersection(&base_cps)
        .filter(|cp| !excluded.contains(cp))
        .count();

    eprintln!(
        "merge_fonts: WARNING — emitting base bytes verbatim (full merge \
         not yet implemented). cmap diff: {sub_only} sub-only codepoints \
         would be added, {sub_overrides} would override base, {} excluded \
         by sub.exclude_codepoints. Sub font ({}) glyphs are NOT yet \
         copied into the output.",
        excluded.len(),
        sub.path.display(),
    );

    // Touch every config field so the call site in `build_one` can't drift
    // away from the merge_fonts signature without us noticing here.
    let _ = (
        sub.scale,
        sub.baseline_offset,
        base.scale,
        base.baseline_offset,
        &output.family_name,
        &output.weight,
        &output.italic,
        &output.width,
        &output.metrics_source,
        &output.metadata_mode,
        &output.manufacturer,
        &output.manufacturer_url,
    );

    if let Some(parent) = export.font_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create output directory: {}", parent.display()))?;
    }
    std::fs::write(&export.font_path, &base_bytes)
        .with_context(|| format!("write merged font: {}", export.font_path.display()))?;

    Ok(())
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
            out.push(parse_one(spec).with_context(|| format!("invalid codepoint spec {spec:?}"))?);
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
    u32::from_str_radix(hex, 16).map_err(|e| anyhow!("invalid hex in {s:?}: {e}"))
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
        let got = parse_codepoint_spec(&["U+0041".into(), "U+2460-U+2462".into()]).unwrap();
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
