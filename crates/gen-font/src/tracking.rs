//! Per-glyph tracking — widen each glyph's advance width and split the
//! resulting gap evenly between left and right sidebearings.
//!
//! Port of `_apply_tracking` from `source/src/font/build.py` (lines 482-510).
//!
//! Adding tracking to a glyph means growing its advance by `t` and nudging
//! the LSB by `t / 2` so the same outline sits centred in the new wider slot
//! — half the new whitespace ends up on the left sidebearing, the other
//! half on the right. Matches how design apps interpret tracking in Latin
//! typography, applied per-glyph rather than as a global text-engine setting.
//!
//! Zero-width glyphs (combining marks, mark-positioning anchors) are skipped
//! so they keep their placement-only role intact.
//!
//! When `tracking_kana` is `Some(v)`, hiragana / katakana / punctuation glyphs
//! receive `v` instead of `tracking`. The Gen Interface JP families use this
//! to give kana / punctuation a slightly looser rhythm than Latin — kana need
//! more breathing room at small sizes against denser Han ideographs.

use anyhow::Context;
use skrifa::{raw::TableProvider, FontRef};
use write_fonts::{
    from_obj::ToOwnedTable,
    tables::{
        hhea::Hhea,
        hmtx::{Hmtx, LongMetric},
    },
    FontBuilder,
};

use crate::classify;

/// Apply per-glyph tracking to every glyph in the font, in place.
///
/// Reads the source `hmtx` / `hhea` / glyph-name tables from `font`,
/// applies the tracking algorithm (see module docs), and writes the
/// updated `hmtx` + `hhea` back into `builder` via [`FontBuilder::add_table`].
///
/// `font` is the read-side handle on the font we're mutating; `builder`
/// is the write-side accumulator that the caller will eventually
/// [`FontBuilder::build`] into the output bytes. The two are kept in sync
/// elsewhere in the pipeline (the builder is seeded from the same bytes
/// `font` parses), so the new `hmtx` / `hhea` we add here override any
/// previous entries in the builder for those tags.
pub fn apply_tracking(
    font: &FontRef<'_>,
    builder: &mut FontBuilder<'_>,
    tracking: i32,
    tracking_kana: Option<i32>,
) -> anyhow::Result<()> {
    // ---- Read source hmtx + hhea ------------------------------------------
    //
    // `hmtx` stores `number_of_h_metrics` long-metric records (advance + lsb)
    // followed by a tail of bare lsb values that share the LAST long metric's
    // advance. The Python reference assigns `(aw, lsb)` per glyph via
    // fontTools' `hmtx[glyph_name] = ...`, which on serialisation expands
    // every touched glyph into a long-metric. Since we touch every glyph
    // with non-zero advance, we materialise a full `num_glyphs`-long
    // `LongMetric` array, mutate, then write back with empty
    // `left_side_bearings` and `number_of_h_metrics = num_glyphs`.
    let hmtx = font.hmtx().context("read hmtx")?;
    let hhea_src = font.hhea().context("read hhea")?;
    let maxp = font.maxp().context("read maxp")?;
    let num_glyphs = u32::from(maxp.num_glyphs());
    let num_long = usize::from(hhea_src.number_of_h_metrics());
    let h_metrics = hmtx.h_metrics();
    let lsb_tail = hmtx.left_side_bearings();

    // Last-long-metric advance is the implicit advance for trailing glyphs.
    let trailing_advance = h_metrics.last().map_or(0, |m| m.advance());

    // Precompute the set of glyph ids that should receive `tracking_kana`
    // by walking the cmap directly. This is more robust than parsing
    // glyph names — fonts like Noto Sans JP ship a format-3 `post` table
    // with no name list, so glyph-name parsing returns synthesised
    // `gidNNN` placeholders that don't carry codepoint information.
    let kana_set = classify::get_kana_or_punct_glyphs(font);

    let mut new_metrics: Vec<LongMetric> = Vec::with_capacity(num_glyphs as usize);
    for gid in 0..num_glyphs {
        // Resolve current (advance, lsb). For gid >= num_long, the advance
        // is the trailing one and the lsb comes from `lsb_tail`.
        let (aw, lsb): (u16, i16) = if (gid as usize) < num_long {
            let m = &h_metrics[gid as usize];
            (m.advance(), m.side_bearing())
        } else {
            let tail_idx = (gid as usize).saturating_sub(num_long);
            let lsb = lsb_tail.get(tail_idx).map_or(0, |sb| sb.get());
            (trailing_advance, lsb)
        };

        if aw == 0 {
            // Skip zero-width glyphs (combining marks etc.) — preserve
            // their placement-only role exactly as the Python does.
            new_metrics.push(LongMetric {
                advance: aw,
                side_bearing: lsb,
            });
            continue;
        }

        // Pick per-glyph tracking value via the cmap-derived kana set.
        let t = match tracking_kana {
            Some(tk) if kana_set.contains(&gid) => tk,
            _ => tracking,
        };

        // Integer half (Python uses floor division; only non-negative
        // tracking is ever passed by the build pipeline, so truncated
        // and floored division agree — see the half_is_floor_div_two test).
        let half = t / 2;

        // Apply (advance += t, lsb += half). Saturate at the integer
        // bounds: u16 advance / i16 lsb. Real-world tracking values are
        // small (single-digit to low-hundreds of design units) and
        // saturation is a safety net rather than an expected branch.
        let new_advance = (i32::from(aw) + t).clamp(0, i32::from(u16::MAX)) as u16;
        let new_lsb =
            (i32::from(lsb) + half).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;

        new_metrics.push(LongMetric {
            advance: new_advance,
            side_bearing: new_lsb,
        });
    }

    // ---- Write hmtx + hhea back into the builder --------------------------
    let new_hmtx = Hmtx {
        h_metrics: new_metrics,
        left_side_bearings: Vec::new(),
    };

    // Rebuild hhea: same fields as the source, but `number_of_h_metrics`
    // grows to `num_glyphs` since we expanded every glyph into a long
    // metric. fontTools recomputes `advance_width_max` /
    // `min_left_side_bearing` / etc. on save; mirroring that here would
    // require a full glyf walk to find xMax/xMin, which is out of scope
    // for the tracking pass — the proportionalisation pipeline runs other
    // metric-touching steps after this one and a single hhea recalc at the
    // end of stage 2 is the right place for it. For now we keep the
    // source values, matching fontTools' default behaviour when only
    // `recalcBBoxes=False` is set on the hmtx assignment path.
    let mut new_hhea: Hhea = hhea_src.to_owned_table();
    new_hhea.number_of_h_metrics = u16::try_from(num_glyphs)
        .context("num_glyphs exceeds u16::MAX — hhea.number_of_h_metrics overflow")?;

    builder
        .add_table(&new_hmtx)
        .map_err(|e| anyhow::anyhow!("add hmtx: {e}"))?;
    builder
        .add_table(&new_hhea)
        .map_err(|e| anyhow::anyhow!("add hhea: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn half_is_floor_div_two() {
        // The Python uses `t // 2` (integer floor division). Rust's `/` on
        // signed ints is truncated-toward-zero, equivalent for non-negative
        // values. The build pipeline only ever passes non-negative tracking,
        // so floor and truncation agree.
        assert_eq!(5 / 2, 2);
        assert_eq!(10 / 2, 5);
        assert_eq!(0 / 2, 0);
    }
}
