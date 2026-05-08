//! Glyph-mutation primitives — translate / shift TrueType glyphs in place.
//!
//! Port of `_shift_glyph_x` from `source/src/font/proportional.py` (lines
//! 217-241). Composite glyphs are shifted by adjusting each component's anchor
//! offset rather than recursing into the referenced glyph — that keeps the
//! underlying base glyph shareable with other composites and avoids
//! double-shifting when both a base and a composite-of-base appear in the
//! same call sequence.
//!
//! NOTE: write-fonts 0.42 does not expose a public mutator for
//! `CompositeGlyph::components` (the field is private and only `components()
//! -> &[Component]` is public). We work around this by rebuilding the
//! composite via [`CompositeGlyph::try_from_iter`]. That path drops any
//! TrueType instructions attached to the composite (`_instructions`, which is
//! also private) — for our pipeline this is fine because Stage 1 produces
//! static composites without per-composite hinting, but it's recorded as a
//! known limitation in case a future caller relies on composite instructions.

use read_fonts::tables::glyf::CurvePoint;
use write_fonts::tables::glyf::{Anchor, Bbox, Component, CompositeGlyph, Glyph};

/// Translate a TrueType glyph horizontally by `dx` in place.
///
/// The bounding box (xMin / xMax) is updated to match. yMin/yMax are
/// unaffected — this is x-only.
///
/// Coordinates are stored as `i16` in the `glyf` table; arithmetic uses
/// saturating addition so that pathological inputs clamp at `i16::MIN/MAX`
/// rather than wrap. Realistic glyph coordinates and reasonable `dx` values
/// stay well within `i16` range.
pub fn shift_glyph_x(glyph: &mut Glyph, dx: i32) {
    if dx == 0 {
        return;
    }
    let dx16: i16 = dx.clamp(i16::MIN as i32, i16::MAX as i32) as i16;

    match glyph {
        Glyph::Empty => {}
        Glyph::Simple(simple) => {
            for contour in &mut simple.contours {
                // Contour wraps Vec<CurvePoint> with a private field; the
                // public From impls round-trip through Vec<CurvePoint>, which
                // is the only mutation path on 0.42.
                let owned = std::mem::take(contour);
                let mut points: Vec<CurvePoint> = owned.into();
                for pt in &mut points {
                    pt.x = pt.x.saturating_add(dx16);
                }
                *contour = points.into();
            }
            simple.bbox.x_min = simple.bbox.x_min.saturating_add(dx16);
            simple.bbox.x_max = simple.bbox.x_max.saturating_add(dx16);
        }
        Glyph::Composite(composite) => {
            let old_bbox = composite.bbox;
            let new_bbox = Bbox {
                x_min: old_bbox.x_min.saturating_add(dx16),
                y_min: old_bbox.y_min,
                x_max: old_bbox.x_max.saturating_add(dx16),
                y_max: old_bbox.y_max,
            };
            // Rebuild components with shifted anchors. `Anchor::Point` refers
            // to a parent/child point index, not a coordinate, so it is not
            // shifted (matches the semantics — point-anchored components ride
            // along with the base they reference).
            let rebuilt: Vec<(Component, Bbox)> = composite
                .components()
                .iter()
                .map(|c| {
                    let mut shifted = c.clone();
                    if let Anchor::Offset { x, y } = shifted.anchor {
                        shifted.anchor = Anchor::Offset {
                            x: x.saturating_add(dx16),
                            y,
                        };
                    }
                    (shifted, new_bbox)
                })
                .collect();
            // try_from_iter only fails on an empty iterator; a `Glyph::Composite`
            // with zero components is invalid input we never produce.
            *composite = CompositeGlyph::try_from_iter(rebuilt)
                .expect("composite glyph already had >= 1 component");
            composite.bbox = new_bbox;
        }
    }
}
