//! Per-glyph side-bearing tweaks.
//!
//! Rust port of `_apply_glyph_spacing` from `source/src/font/build.py`.
//! Stage 2.3 of the proportionalisation pipeline — a manual fallback that
//! layers fixed `(lsb_delta, rsb_delta)` shifts on top of palt + tracking
//! for the rare glyph whose sidebearings still read off after the
//! canonical metrics passes.
//!
//! - `lsb_delta` shifts the outline `lsb_delta` units to the right *inside*
//!   the slot and grows advance by the same amount, so the whitespace between
//!   the slot's left edge and the outline grows by `lsb_delta` while the
//!   right side stays untouched.
//! - `rsb_delta` extends the slot on the right by `rsb_delta` units without
//!   moving the outline.
//!
//! Combined effect: `advance += lsb_delta + rsb_delta`, `lsb += lsb_delta`.
//! Outline coordinates are NEVER touched.
//!
//! Apply sparingly and AFTER `apply_tracking` so the deltas layer on top of
//! the canonical proportional metrics.

use anyhow::bail;
use write_fonts::FontBuilder;

/// Adjust per-glyph left / right sidebearings. Returns the count of glyphs
/// actually adjusted.
///
/// Glyphs whose codepoint is absent from the cmap and zero-advance glyphs
/// (combining marks, mark anchors) are skipped silently.
pub fn apply_glyph_spacing(
    builder: &mut FontBuilder<'_>,
    spacing: &[(char, (i32, i32))],
) -> anyhow::Result<usize> {
    let _ = builder;
    if spacing.is_empty() {
        return Ok(0);
    }
    bail!(
        "apply_glyph_spacing: TODO(impl) — cmap+hmtx mutation against \
         write-fonts 0.42 surface is not yet wired up"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::FontBuilder;

    #[test]
    fn empty_spacing_returns_zero() {
        let mut builder = FontBuilder::new();
        assert_eq!(apply_glyph_spacing(&mut builder, &[]).unwrap(), 0);
    }
}
