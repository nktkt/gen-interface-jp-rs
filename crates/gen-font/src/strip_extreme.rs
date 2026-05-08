//! Bbox / head-table cleanup: neutralise outlier glyphs that dominate
//! `head.yMax` / `head.yMin`.
//!
//! Port of `_strip_extreme_glyphs` from `source/src/font/build.py` (Stage 2.4
//! of the build pipeline). See `CLAUDE.md` ("Vertical metrics policy") and
//! `docs/ARCHITECTURE.md` ("Illustrator box problem") for the rationale.
//!
//! Targets vertical-text-only glyphs (kana iteration marks 〱〲 and their
//! `vert`/`vrt2` alternates) that inflate head.yMax/yMin. Illustrator's text
//! frame auto-sizing reads head bbox, so these outliers force frames open
//! with several extra hundred units of vertical padding even on plain Latin.
//! Acceptable trade-off for a horizontal-only UI font.
//!
//! Removing the slot outright would shift every later index in GSUB / GPOS
//! lookups. Instead we keep the slot in place and only replace the outline
//! with an empty Glyph — the bbox no longer contributes to head, and dropping
//! the cmap entry makes the codepoint fall through to .notdef when typed.

use anyhow::bail;
use write_fonts::FontBuilder;

/// Threshold for "extreme" glyphs whose bbox dominates head.yMax/yMin.
///
/// em=1000 base; the legitimate Latin/CJK content of Noto stays well within
/// these bounds, so anything past them is the vertical-only iteration-mark
/// glyphs we want to neutralise.
pub const EXTREME_YMAX: i32 = 1200;
pub const EXTREME_YMIN: i32 = -400;

/// Neutralise glyphs whose bbox extends far beyond the em-square. Returns the
/// number of glyphs that were neutralised.
pub fn strip_extreme_glyphs(builder: &mut FontBuilder<'_>) -> anyhow::Result<usize> {
    let _ = builder;
    bail!(
        "strip_extreme_glyphs: TODO(impl) — glyf bbox walk + cmap/GSUB \
         pruning against write-fonts 0.42 surface is not yet wired up"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extreme_thresholds_are_canonical() {
        assert_eq!(EXTREME_YMAX, 1200);
        assert_eq!(EXTREME_YMIN, -400);
    }

    #[test]
    fn extreme_band_brackets_em_square() {
        // em=1000, Latin/CJK content lives roughly within ±1000.
        assert!(EXTREME_YMAX > 1000);
        assert!(EXTREME_YMIN < 0);
    }
}
