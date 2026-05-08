//! Bake the OpenType `palt` GPOS feature into static `hmtx` metrics so
//! the font reads as proportional everywhere — even in apps that don't
//! enable `palt`.
//!
//! # Why
//!
//! CJK fonts ship with full-width metrics by default — every glyph occupies
//! the same em-square box regardless of its actual outline width — and rely
//! on the GPOS `palt` feature to optically narrow kana, punctuation, and
//! Latin-in-CJK glyphs at runtime. Apps that don't enable `palt` (Adobe's
//! Japanese composer, browser fallbacks, anything treating CJK as monospaced
//! for layout) miss those adjustments and lay text out at full-width spacing.
//!
//! This module bakes `palt` adjustments into the static `hmtx` so the font
//! reads as proportional everywhere, then removes the now-redundant
//! `palt`/`vpal`/`halt`/`vhal` features to prevent apps that *do* honour them
//! from double-applying.
//!
//! # Three buckets
//!
//! - **Full palt** — glyphs in `palt_adjustments` not called out in
//!   `reduced_palt`. The XPlacement / XAdvance from palt is applied at full
//!   strength.
//! - **Reduced palt** (`reduced_palt`) — same glyphs as palt but the
//!   adjustment is scaled by `reduced_palt_scale` (default 1/3). Used in
//!   this project for punctuation, where full palt feels too tight when set
//!   against kana that already shrank.
//! - **Squeeze SB** (`squeeze_sb`) — glyphs *without* palt entries that
//!   should still narrow proportionally. Their LSB and RSB each shrink by
//!   `(1 - squeeze_sb_scale)` so the rhythm stays consistent.
//!
//! Only TrueType-outlined fonts are supported — palt baking writes back to
//! `glyf`, not to CFF.
//!
//! Port of `make_proportional` from `source/src/font/proportional.py`
//! (lines 43-150).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::bail;
use write_fonts::FontBuilder;

/// Default scale for the reduced-palt bucket (e.g. punctuation): a third of
/// the full palt shift.
pub const DEFAULT_REDUCED_PALT_SCALE: f32 = 1.0 / 3.0;

/// Caller-tuneable knobs for [`make_proportional`].
#[derive(Debug, Clone)]
pub struct ProportionalOptions {
    /// Glyphs in this set with palt entries get a fraction of the adjustment
    /// (default 1/3) — used for punctuation.
    pub reduced_palt: Option<BTreeSet<String>>,
    /// Scale applied to palt XPlacement / XAdvance for `reduced_palt` glyphs.
    pub reduced_palt_scale: f32,
    /// Glyphs *without* palt entries that should still narrow proportionally:
    /// LSB and RSB each shrink by `1 - squeeze_sb_scale`.
    pub squeeze_sb: Option<BTreeSet<String>>,
    /// Defaults to `reduced_palt_scale` when `None`.
    pub squeeze_sb_scale: Option<f32>,
    /// Caller-supplied palt table to use instead of reading from the font.
    /// Useful when variable-instantiation has corrupted the font's own palt
    /// — variable→static baking can leave palt ValueRecords with zeroed
    /// XPlacement/XAdvance pairs.
    pub palt_override: Option<BTreeMap<String, (i32, i32)>>,
}

impl Default for ProportionalOptions {
    fn default() -> Self {
        Self {
            reduced_palt: None,
            reduced_palt_scale: DEFAULT_REDUCED_PALT_SCALE,
            squeeze_sb: None,
            squeeze_sb_scale: None,
            palt_override: None,
        }
    }
}

/// Bake palt adjustments into hmtx in place, then strip prop features.
///
/// See module docs for the three-bucket policy. Only TrueType-outlined fonts
/// are supported (palt baking writes glyf).
pub fn make_proportional(
    builder: &mut FontBuilder<'_>,
    opts: ProportionalOptions,
) -> anyhow::Result<()> {
    let _ = (builder, opts);
    // TODO(impl): the per-glyph palt baking + squeeze-SB sidebearing rewrite
    // requires hmtx/glyf mutation against verified write-fonts 0.42 surface.
    // The algorithm itself (mirroring `proportional.py:43-150`) is captured
    // at the module level; the body is wired through `bail!` so callers
    // see a clear error rather than a silent no-op.
    bail!(
        "make_proportional: TODO(impl) — palt baking against \
         write-fonts 0.42 hmtx/glyf surface is not yet wired up"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_reduced_palt_scale() {
        let opts = ProportionalOptions::default();
        assert!((opts.reduced_palt_scale - 1.0 / 3.0).abs() < 1e-7);
    }

    #[test]
    fn default_other_fields_are_none() {
        let opts = ProportionalOptions::default();
        assert!(opts.reduced_palt.is_none());
        assert!(opts.squeeze_sb.is_none());
        assert!(opts.squeeze_sb_scale.is_none());
        assert!(opts.palt_override.is_none());
    }
}
