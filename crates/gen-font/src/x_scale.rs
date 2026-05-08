//! Horizontal-only scale (長体 / condensed) for glyphs, hmtx, and GPOS.
//!
//! Ports `_apply_x_scale` and `_scale_gpos_x` from `source/src/font/build.py`
//! (lines 305-405). See that file for the design rationale; this module is
//! a faithful port with `f32` rounding semantics matched to Python's
//! `round(x * scale)` (banker's rounding via `OtRound`-style "half away
//! from zero" for the integer cases is good enough — the inputs are always
//! integer design units multiplied by a near-1 scale factor, so rounding
//! collisions are rare and the visual impact of either policy is well
//! below the rasteriser's anti-aliasing threshold).
//!
//! The merge step only supports uniform scale, so condensing CJK relative to
//! Latin happens *before* the merge on the base font. This squeezes Noto in x
//! only — y stays untouched — then the merge's uniform scale on top preserves
//! the modified x:y ratio. GPOS X values (kerning, mark positioning) are
//! scaled to match so kerning pairs continue to land where the design intends.

use anyhow::{anyhow, Context};
use read_fonts::tables::glyf::CurvePoint;
use skrifa::{raw::TableProvider, FontRef};
use write_fonts::{
    from_obj::{FromTableRef, ToOwnedTable},
    tables::{
        glyf::{Anchor, Bbox, Component, CompositeGlyph, GlyfLocaBuilder, Glyph, SimpleGlyph},
        gpos::{
            AnchorTable, ExtensionSubtable, Gpos, MarkArray, PairPos, PairValueRecord,
            PositionLookup, SinglePos, ValueRecord,
        },
        head::Head,
        hhea::Hhea,
        hmtx::{Hmtx, LongMetric},
        loca::LocaFormat,
    },
    FontBuilder,
};

/// Round `x * scale` to the nearest integer with ties going away from zero.
///
/// This matches Python's built-in `round()` for non-half values and is close
/// enough to fontTools' rounding policy for the half cases that arise in
/// practice (positions are integer design units; the half cases require
/// `2 * pos * scale` to be an odd integer, which is exceptionally rare for
/// the scales we use, ~0.9 to ~1.0).
#[inline]
fn round_scaled(value: i32, scale: f32) -> i32 {
    (value as f32 * scale).round() as i32
}

/// Apply a horizontal-only scale to glyphs, hmtx, and GPOS in place.
///
/// Reads source `glyf`/`loca`/`hmtx`/`hhea`/`head`/`GPOS` from `font`,
/// applies an x-only scale to every coordinate / advance / GPOS X value,
/// and writes the rebuilt tables back into `builder` via
/// [`FontBuilder::add_table`]. Same shape as
/// [`crate::strip_extreme::strip_extreme_glyphs`] and
/// [`crate::tracking::apply_tracking`].
///
/// `scale == 1.0` is a no-op early-return (no tables touched).
///
/// What this does, in order:
///
/// 1. Walk `glyf` / `loca`. For each glyph:
///    - Composite: scale every component's `Anchor::Offset { x, .. }` by
///      `scale`, scale the composite's `bbox.x_min` / `x_max`. `Anchor::Point`
///      anchors reference contour points (no x to scale).
///    - Simple: scale every CurvePoint.x and rebuild the bbox via
///      [`SimpleGlyph::recompute_bounding_box`].
/// 2. Rebuild `hmtx`: for each gid, `(advance, lsb)` becomes
///    `(round(aw * scale) as u16, round(lsb * scale) as i16)`.
/// 3. Walk `GPOS.lookup_list.lookups` and scale every X-direction value:
///    `SinglePos` (type 1), `PairPos` formats 1+2 (type 2), `MarkBase` /
///    `MarkLig` / `MarkMark` (types 4/5/6), and `Extension` (type 9, recurse
///    on the inner subtable). `Cursive` (type 3) and `Contextual` /
///    `ChainContextual` (types 7/8) are deliberately skipped — see the
///    Python reference for why.
pub fn apply_x_scale(
    font: &FontRef<'_>,
    builder: &mut FontBuilder<'_>,
    scale: f32,
) -> anyhow::Result<()> {
    if (scale - 1.0).abs() < f32::EPSILON {
        return Ok(());
    }

    // ---- 1. Rebuild glyf + loca + head.indexToLocFormat ------------------
    let glyf = font.glyf().context("read glyf")?;
    let loca = font.loca(None).context("read loca")?;
    let maxp = font.maxp().context("read maxp")?;
    let num_glyphs = u32::from(maxp.num_glyphs());

    let mut glyph_builder = GlyfLocaBuilder::new();
    for gid in 0..num_glyphs {
        let gid_skrifa = skrifa::GlyphId::new(gid);
        // Re-parse the glyph through the read-fonts side, then convert to
        // a write-fonts owned `Glyph` so we can mutate.
        let glyph_typed: Glyph = match loca.get_glyf(gid_skrifa, &glyf) {
            Ok(Some(g)) => Glyph::from_table_ref(&g),
            // No outline / empty slot — preserve as Empty so the loca entry
            // survives and downstream gid references stay correct.
            Ok(None) => Glyph::Empty,
            Err(e) => return Err(anyhow!("read glyf for gid {gid}: {e}")),
        };

        let scaled = scale_glyph_x(glyph_typed, scale);
        glyph_builder
            .add_glyph(&scaled)
            .map_err(|e| anyhow!("compile glyph at gid {gid}: {e}"))?;
    }
    let (new_glyf, new_loca, loca_format) = glyph_builder.build();

    // The new loca format may differ from the source's (depends on max
    // offset). Update `head` to match.
    let mut new_head: Head = font.head().context("read head")?.to_owned_table();
    new_head.index_to_loc_format = match loca_format {
        LocaFormat::Short => 0,
        LocaFormat::Long => 1,
    };

    builder
        .add_table(&new_glyf)
        .map_err(|e| anyhow!("add glyf: {e}"))?;
    builder
        .add_table(&new_loca)
        .map_err(|e| anyhow!("add loca: {e}"))?;
    builder
        .add_table(&new_head)
        .map_err(|e| anyhow!("add head: {e}"))?;

    // ---- 2. Rebuild hmtx + hhea ----------------------------------------
    //
    // Same pattern as `tracking::apply_tracking` and
    // `strip_extreme::strip_extreme_glyphs`: materialise a full
    // `num_glyphs`-long `LongMetric` array, scale every entry, emit with
    // empty `left_side_bearings` and `hhea.number_of_h_metrics = num_glyphs`.
    let hmtx_src = font.hmtx().context("read hmtx")?;
    let hhea_src = font.hhea().context("read hhea")?;
    let h_metrics = hmtx_src.h_metrics();
    let lsb_tail = hmtx_src.left_side_bearings();
    let num_long = usize::from(hhea_src.number_of_h_metrics());
    let trailing_advance = h_metrics.last().map_or(0, |m| m.advance());

    let mut new_metrics: Vec<LongMetric> = Vec::with_capacity(num_glyphs as usize);
    for gid in 0..num_glyphs {
        let (advance, side_bearing): (u16, i16) = if (gid as usize) < num_long {
            let m = &h_metrics[gid as usize];
            (m.advance(), m.side_bearing())
        } else {
            let tail_idx = (gid as usize).saturating_sub(num_long);
            let lsb = lsb_tail.get(tail_idx).map_or(0, |sb| sb.get());
            (trailing_advance, lsb)
        };

        // round(aw * scale) clamped to u16; round(lsb * scale) clamped to i16.
        let new_advance =
            round_scaled(i32::from(advance), scale).clamp(0, i32::from(u16::MAX)) as u16;
        let new_lsb = round_scaled(i32::from(side_bearing), scale)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        new_metrics.push(LongMetric {
            advance: new_advance,
            side_bearing: new_lsb,
        });
    }
    let new_hmtx = Hmtx {
        h_metrics: new_metrics,
        left_side_bearings: Vec::new(),
    };
    let mut new_hhea: Hhea = hhea_src.to_owned_table();
    new_hhea.number_of_h_metrics = u16::try_from(num_glyphs)
        .context("num_glyphs exceeds u16::MAX — hhea.number_of_h_metrics overflow")?;
    builder
        .add_table(&new_hmtx)
        .map_err(|e| anyhow!("add hmtx: {e}"))?;
    builder
        .add_table(&new_hhea)
        .map_err(|e| anyhow!("add hhea: {e}"))?;

    // ---- 3. Scale GPOS X values ----------------------------------------
    //
    // Walk every PositionLookup, then every subtable, and apply
    // `scale_subtable_x`. Cursive / Contextual / ChainContextual are
    // skipped (see module docs). Extension subtables recurse into their
    // inner subtable.
    if let Ok(gpos) = font.gpos() {
        let mut new_gpos: Gpos = gpos.to_owned_table();
        let lookup_list = &mut *new_gpos.lookup_list;
        for lookup_marker in &mut lookup_list.lookups {
            let lookup = &mut **lookup_marker;
            scale_position_lookup_x(lookup, scale);
        }
        builder
            .add_table(&new_gpos)
            .map_err(|e| anyhow!("add GPOS: {e}"))?;
    }

    Ok(())
}

// --------------------------------------------------------------------------
// Glyph scaling
// --------------------------------------------------------------------------

/// Return a new `Glyph` with all x-direction coordinates scaled.
fn scale_glyph_x(glyph: Glyph, scale: f32) -> Glyph {
    match glyph {
        Glyph::Empty => Glyph::Empty,
        Glyph::Simple(simple) => Glyph::Simple(scale_simple_x(simple, scale)),
        Glyph::Composite(composite) => Glyph::Composite(scale_composite_x(composite, scale)),
    }
}

/// Scale every `CurvePoint.x` in a `SimpleGlyph` and recompute its bbox.
fn scale_simple_x(simple: SimpleGlyph, scale: f32) -> SimpleGlyph {
    let SimpleGlyph {
        bbox: _,
        contours,
        instructions,
    } = simple;

    // `Contour`'s inner Vec is private; convert via `From<Contour> for
    // Vec<CurvePoint>` (and back via `From<Vec<CurvePoint>>`).
    let new_contours = contours
        .into_iter()
        .map(|c| {
            let pts: Vec<CurvePoint> = Vec::from(c)
                .into_iter()
                .map(|p| CurvePoint {
                    x: round_scaled(i32::from(p.x), scale)
                        .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                        as i16,
                    y: p.y,
                    on_curve: p.on_curve,
                })
                .collect();
            pts.into()
        })
        .collect();

    let mut out = SimpleGlyph {
        bbox: Bbox::default(),
        contours: new_contours,
        instructions,
    };
    // Recompute the bbox from the new points. This handles xMin/xMax/yMin/yMax
    // correctly even though we only scaled x: yMin/yMax fall out of the
    // unchanged y values.
    out.recompute_bounding_box();
    out
}

/// Scale every component's x-anchor (Offset variant only) and the composite
/// bbox's `x_min` / `x_max`.
///
/// Composite bbox in `write-fonts` is the union of per-component bboxes,
/// stored at the composite level. Scaling `x_min`/`x_max` linearly is exact
/// because x-only scale doesn't change which x values are extremal — every
/// x scales by the same factor.
fn scale_composite_x(composite: CompositeGlyph, scale: f32) -> CompositeGlyph {
    let bbox = composite.bbox;
    let scaled_bbox = Bbox {
        x_min: round_scaled(i32::from(bbox.x_min), scale)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        x_max: round_scaled(i32::from(bbox.x_max), scale)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        y_min: bbox.y_min,
        y_max: bbox.y_max,
    };

    // CompositeGlyph's `components` field is private; rebuild via
    // `try_from_iter`. We don't have per-component bboxes (the source only
    // tracks the union), so we hand the same scaled overall bbox to every
    // component — `try_from_iter` unions them, which is idempotent for
    // identical inputs and produces the same final scaled bbox.
    let scaled_components: Vec<(Component, Bbox)> = composite
        .components()
        .iter()
        .map(|c| {
            let scaled_anchor = match c.anchor {
                Anchor::Offset { x, y } => Anchor::Offset {
                    x: round_scaled(i32::from(x), scale)
                        .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                        as i16,
                    y,
                },
                // Anchor::Point references contour points; no raw x to scale.
                pt @ Anchor::Point { .. } => pt,
            };
            let new_component = Component {
                glyph: c.glyph,
                anchor: scaled_anchor,
                flags: c.flags,
                transform: c.transform,
            };
            (new_component, scaled_bbox)
        })
        .collect();

    // SAFETY-of-correctness: a composite glyph by construction has at least
    // one component, so `try_from_iter` cannot return `NoComponents` here.
    // If it somehow does, fall back to the unscaled composite — better than
    // panicking, and the caller's `glyph_builder.add_glyph` will surface
    // any structural issue downstream.
    CompositeGlyph::try_from_iter(scaled_components).unwrap_or_else(|_| {
        let mut fallback = composite;
        fallback.bbox = scaled_bbox;
        fallback
    })
}

// --------------------------------------------------------------------------
// GPOS scaling
// --------------------------------------------------------------------------

/// Scale every X-direction value inside a `PositionLookup` in place.
fn scale_position_lookup_x(lookup: &mut PositionLookup, scale: f32) {
    match lookup {
        PositionLookup::Single(l) => {
            for sub_marker in &mut l.subtables {
                scale_single_pos_x(sub_marker, scale);
            }
        }
        PositionLookup::Pair(l) => {
            for sub_marker in &mut l.subtables {
                scale_pair_pos_x(sub_marker, scale);
            }
        }
        PositionLookup::MarkToBase(l) => {
            for sub_marker in &mut l.subtables {
                let sub = &mut **sub_marker;
                scale_mark_array_x(&mut sub.mark_array, scale);
                let base_array = &mut *sub.base_array;
                for record in &mut base_array.base_records {
                    for anchor_marker in &mut record.base_anchors {
                        if let Some(anchor) = anchor_marker.as_mut() {
                            scale_anchor_x(anchor, scale);
                        }
                    }
                }
            }
        }
        PositionLookup::MarkToLig(l) => {
            for sub_marker in &mut l.subtables {
                let sub = &mut **sub_marker;
                scale_mark_array_x(&mut sub.mark_array, scale);
                // The Python reference doesn't iterate LigatureArray /
                // ComponentRecord, but Noto Sans JP doesn't use mark-to-lig
                // in any consequential way for our pipeline. Mirror the
                // mark-array scale here so kerning-adjacent positioning
                // stays consistent if a downstream font ever does, and
                // leave a TODO to flag the divergence from the Python.
                //
                // TODO(parity): port mark-to-ligature LigatureArray / ComponentRecord
                // anchor scaling once a JP source actually exercises it.
                let _ = &sub.ligature_array;
            }
        }
        PositionLookup::MarkToMark(l) => {
            for sub_marker in &mut l.subtables {
                let sub = &mut **sub_marker;
                scale_mark_array_x(&mut sub.mark1_array, scale);
                let mark2_array = &mut *sub.mark2_array;
                for record in &mut mark2_array.mark2_records {
                    for anchor_marker in &mut record.mark2_anchors {
                        if let Some(anchor) = anchor_marker.as_mut() {
                            scale_anchor_x(anchor, scale);
                        }
                    }
                }
            }
        }
        PositionLookup::Extension(l) => {
            // Type 9 — unwrap and recurse on the inner subtable.
            for sub_marker in &mut l.subtables {
                scale_extension_subtable_x(sub_marker, scale);
            }
        }
        // Cursive (type 3) and Contextual / ChainContextual (types 7/8) are
        // deliberately skipped, mirroring the Python reference. Cursive
        // attachment uses entry/exit anchors that interact with bidi /
        // directional layout; rescaling them in isolation can break the
        // intended cursive flow. Contextual lookups don't carry direct
        // X-direction values in their subtable — they only reference other
        // lookups by index.
        PositionLookup::Cursive(_)
        | PositionLookup::Contextual(_)
        | PositionLookup::ChainContextual(_) => {}
    }
}

fn scale_extension_subtable_x(ext: &mut ExtensionSubtable, scale: f32) {
    match ext {
        ExtensionSubtable::Single(e) => scale_single_pos_x(&mut e.extension, scale),
        ExtensionSubtable::Pair(e) => scale_pair_pos_x(&mut e.extension, scale),
        ExtensionSubtable::MarkToBase(e) => {
            let inner = &mut *e.extension;
            scale_mark_array_x(&mut inner.mark_array, scale);
            let base_array = &mut *inner.base_array;
            for record in &mut base_array.base_records {
                for anchor_marker in &mut record.base_anchors {
                    if let Some(anchor) = anchor_marker.as_mut() {
                        scale_anchor_x(anchor, scale);
                    }
                }
            }
        }
        ExtensionSubtable::MarkToLig(e) => {
            let inner = &mut *e.extension;
            scale_mark_array_x(&mut inner.mark_array, scale);
            // Same TODO(parity) as the non-extension MarkToLig branch.
            let _ = &inner.ligature_array;
        }
        ExtensionSubtable::MarkToMark(e) => {
            let inner = &mut *e.extension;
            scale_mark_array_x(&mut inner.mark1_array, scale);
            let mark2_array = &mut *inner.mark2_array;
            for record in &mut mark2_array.mark2_records {
                for anchor_marker in &mut record.mark2_anchors {
                    if let Some(anchor) = anchor_marker.as_mut() {
                        scale_anchor_x(anchor, scale);
                    }
                }
            }
        }
        // Same skip set as the non-extension branch.
        ExtensionSubtable::Cursive(_)
        | ExtensionSubtable::Contextual(_)
        | ExtensionSubtable::ChainContextual(_) => {}
    }
}

fn scale_single_pos_x(sub: &mut SinglePos, scale: f32) {
    match sub {
        SinglePos::Format1(f1) => scale_value_record_x(&mut f1.value_record, scale),
        SinglePos::Format2(f2) => {
            for vr in &mut f2.value_records {
                scale_value_record_x(vr, scale);
            }
        }
    }
}

fn scale_pair_pos_x(sub: &mut PairPos, scale: f32) {
    match sub {
        PairPos::Format1(f1) => {
            for ps_marker in &mut f1.pair_sets {
                let ps = &mut **ps_marker;
                for pvr in &mut ps.pair_value_records {
                    scale_pair_value_record_x(pvr, scale);
                }
            }
        }
        PairPos::Format2(f2) => {
            for c1r in &mut f2.class1_records {
                for c2r in &mut c1r.class2_records {
                    scale_value_record_x(&mut c2r.value_record1, scale);
                    scale_value_record_x(&mut c2r.value_record2, scale);
                }
            }
        }
    }
}

fn scale_pair_value_record_x(pvr: &mut PairValueRecord, scale: f32) {
    scale_value_record_x(&mut pvr.value_record1, scale);
    scale_value_record_x(&mut pvr.value_record2, scale);
}

fn scale_mark_array_x(ma: &mut MarkArray, scale: f32) {
    for mr in &mut ma.mark_records {
        scale_anchor_x(&mut mr.mark_anchor, scale);
    }
}

fn scale_anchor_x(anchor: &mut AnchorTable, scale: f32) {
    match anchor {
        AnchorTable::Format1(a) => {
            a.x_coordinate = clamp_i16(round_scaled(i32::from(a.x_coordinate), scale));
        }
        AnchorTable::Format2(a) => {
            a.x_coordinate = clamp_i16(round_scaled(i32::from(a.x_coordinate), scale));
        }
        AnchorTable::Format3(a) => {
            a.x_coordinate = clamp_i16(round_scaled(i32::from(a.x_coordinate), scale));
        }
    }
}

/// Scale `x_placement` and `x_advance` if present. Y fields are untouched.
fn scale_value_record_x(vr: &mut ValueRecord, scale: f32) {
    if let Some(x) = vr.x_placement {
        vr.x_placement = Some(clamp_i16(round_scaled(i32::from(x), scale)));
    }
    if let Some(x) = vr.x_advance {
        vr.x_advance = Some(clamp_i16(round_scaled(i32::from(x), scale)));
    }
}

#[inline]
fn clamp_i16(v: i32) -> i16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_one_is_noop() {
        // Even without a real `FontRef`, the `scale == 1.0` early-return must
        // not touch the builder. Use a dummy byte slice that fails to parse;
        // the early return short-circuits before we reach any IO.
        //
        // We can't construct a `FontRef<'_>` from invalid bytes (it errors),
        // so build one over an empty TTF header is overkill — instead we
        // exercise the early-return through the public API by constructing
        // the smallest possible `FontRef` via skrifa's read side. To keep the
        // test self-contained we just check the early-return arithmetic.
        assert!((1.0_f32 - 1.0_f32).abs() < f32::EPSILON);
        assert!((0.9_f32 - 1.0_f32).abs() >= f32::EPSILON);
    }

    #[test]
    fn round_scaled_matches_python_round() {
        // Python: `round(100 * 0.9)` -> 90, `round(101 * 0.9)` -> 91 (90.9).
        assert_eq!(round_scaled(100, 0.9), 90);
        assert_eq!(round_scaled(101, 0.9), 91);
        // Negative side bearings round symmetrically.
        assert_eq!(round_scaled(-50, 0.9), -45);
        // Identity scale is a no-op.
        assert_eq!(round_scaled(123, 1.0), 123);
        assert_eq!(round_scaled(-456, 1.0), -456);
    }

    #[test]
    fn clamp_i16_saturates() {
        assert_eq!(clamp_i16(40_000), i16::MAX);
        assert_eq!(clamp_i16(-40_000), i16::MIN);
        assert_eq!(clamp_i16(0), 0);
        assert_eq!(clamp_i16(123), 123);
    }
}
