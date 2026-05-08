//! Horizontal-only scale (長体 / condensed) for glyphs, hmtx, and GPOS.
//!
//! Ports `_apply_x_scale` and `_scale_gpos_x` from `source/src/font/build.py`.
//!
//! The merge step only supports uniform scale, so condensing CJK relative to
//! Latin happens *before* the merge on the base font. This squeezes Noto in x
//! only — y stays untouched — then the merge's uniform scale on top preserves
//! the modified x:y ratio. GPOS X values (kerning, mark positioning) are
//! scaled to match so kerning pairs continue to land where the design intends.

use anyhow::bail;
use write_fonts::FontBuilder;

/// Apply a horizontal-only scale to glyphs, hmtx, and GPOS in place.
///
/// `scale == 1.0` is a no-op early-return.
pub fn apply_x_scale(builder: &mut FontBuilder<'_>, scale: f32) -> anyhow::Result<()> {
    if (scale - 1.0).abs() < f32::EPSILON {
        return Ok(());
    }
    let _ = builder;
    bail!(
        "apply_x_scale: TODO(impl) — glyf/hmtx/GPOS X-direction scale walk \
         against write-fonts 0.42 surface is not yet wired up"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::FontBuilder;

    #[test]
    fn scale_one_is_noop() {
        let mut builder = FontBuilder::new();
        assert!(apply_x_scale(&mut builder, 1.0).is_ok());
    }
}
