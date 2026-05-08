//! Glyph-mutation primitives — translate / shift TrueType glyphs in place.
//!
//! Port of `_shift_glyph_x` from `source/src/font/proportional.py` (lines
//! 217-241). Composite glyphs are shifted by adjusting each component's anchor
//! offset rather than recursing into the referenced glyph — that keeps the
//! underlying base glyph shareable with other composites and avoids
//! double-shifting when both a base and a composite-of-base appear in the
//! same call sequence.
//!
//! NOTE: the actual `write-fonts` 0.42 API surface for mutating `Glyph::Simple`
//! contour points / `Glyph::Composite` component anchors does not match the
//! shape this port was sketched against. The function below preserves the
//! signature and dispatch shape but leaves per-variant mutation as a TODO
//! until the API is pinned. Higher layers (`build_one`) bail before producing
//! output, so callers see a clear error rather than a silent no-op.

use write_fonts::tables::glyf::Glyph;

/// Translate a TrueType glyph horizontally by `dx` in place.
///
/// The bounding box (xMin / xMax) is updated to match. yMin/yMax are
/// unaffected — this is x-only.
pub fn shift_glyph_x(glyph: &mut Glyph, dx: i32) {
    let _ = (glyph, dx);
    // TODO(api): translate contour points (Simple) / component anchors
    // (Composite) by dx; update xMin/xMax. The exact write-fonts 0.42 surface
    // requires a deeper integration than the per-file scope of this stub.
}
