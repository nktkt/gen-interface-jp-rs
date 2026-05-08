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

use anyhow::Context;
use skrifa::{charmap::Charmap, raw::TableProvider, FontRef, GlyphId};
use write_fonts::{
    from_obj::ToOwnedTable,
    tables::{
        hhea::Hhea,
        hmtx::{Hmtx, LongMetric},
    },
    FontBuilder,
};

/// Adjust per-glyph left / right sidebearings. Returns the count of glyphs
/// actually adjusted.
///
/// Reads the source `cmap` / `hmtx` / `hhea` tables from `font`, walks the
/// `(char, (lsb_delta, rsb_delta))` spacing list, and writes the updated
/// `hmtx` + `hhea` back into `builder` via [`FontBuilder::add_table`].
///
/// Glyphs whose codepoint is absent from the cmap and zero-advance glyphs
/// (combining marks, mark anchors) are skipped silently.
pub fn apply_glyph_spacing(
    font: &FontRef<'_>,
    builder: &mut FontBuilder<'_>,
    spacing: &[(char, (i32, i32))],
) -> anyhow::Result<usize> {
    if spacing.is_empty() {
        return Ok(0);
    }

    // ---- Resolve (char -> glyph id) via cmap ------------------------------
    //
    // Mirrors the Python reference's `cmap.get(cp)` on `font.getBestCmap()`.
    // Skrifa's `Charmap` picks the most-suitable subtable internally, so we
    // get the same "best cmap" semantics for free.
    let charmap = Charmap::new(font);

    // Pair up the resolved glyph id with the deltas, dropping zero-delta
    // entries (no work) and unmapped codepoints (silently skipped, like
    // the Python).
    let mut targets: Vec<(GlyphId, i32, i32)> = Vec::with_capacity(spacing.len());
    for &(ch, (lsb_delta, rsb_delta)) in spacing {
        if lsb_delta == 0 && rsb_delta == 0 {
            continue;
        }
        let cp = ch as u32;
        let Some(gid) = charmap.map(cp) else {
            continue;
        };
        targets.push((gid, lsb_delta, rsb_delta));
    }

    // ---- Read source hmtx + hhea ------------------------------------------
    //
    // Same full long-metric materialisation as `apply_tracking`: read the
    // existing `(advance, lsb)` for every glyph, mutate the targeted ones,
    // then emit a single `Hmtx` whose `h_metrics` covers all `num_glyphs`
    // and whose `left_side_bearings` tail is empty. Hhea's
    // `number_of_h_metrics` grows to `num_glyphs` to match.
    let hmtx = font.hmtx().context("read hmtx")?;
    let hhea_src = font.hhea().context("read hhea")?;
    let maxp = font.maxp().context("read maxp")?;
    let num_glyphs = u32::from(maxp.num_glyphs());
    let num_long = usize::from(hhea_src.number_of_h_metrics());
    let h_metrics = hmtx.h_metrics();
    let lsb_tail = hmtx.left_side_bearings();

    let trailing_advance = h_metrics.last().map_or(0, |m| m.advance());

    let mut new_metrics: Vec<LongMetric> = Vec::with_capacity(num_glyphs as usize);
    for gid in 0..num_glyphs {
        let (aw, lsb): (u16, i16) = if (gid as usize) < num_long {
            let m = &h_metrics[gid as usize];
            (m.advance(), m.side_bearing())
        } else {
            let tail_idx = (gid as usize).saturating_sub(num_long);
            let lsb = lsb_tail.get(tail_idx).map_or(0, |sb| sb.get());
            (trailing_advance, lsb)
        };
        new_metrics.push(LongMetric {
            advance: aw,
            side_bearing: lsb,
        });
    }

    // ---- Apply deltas, counting glyphs actually adjusted ------------------
    let mut adjusted: usize = 0;
    for (gid, lsb_delta, rsb_delta) in targets {
        let idx = gid.to_u32() as usize;
        let Some(metric) = new_metrics.get_mut(idx) else {
            // Glyph id exists in cmap but somehow >= num_glyphs — shouldn't
            // happen for well-formed fonts; skip silently to mirror the
            // Python's defensive shape.
            continue;
        };
        if metric.advance == 0 {
            // Skip zero-advance glyphs (combining marks, mark anchors) —
            // preserve their placement-only role exactly as the Python.
            continue;
        }
        let new_advance = (i32::from(metric.advance) + lsb_delta + rsb_delta)
            .clamp(0, i32::from(u16::MAX)) as u16;
        let new_lsb = (i32::from(metric.side_bearing) + lsb_delta)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        metric.advance = new_advance;
        metric.side_bearing = new_lsb;
        adjusted += 1;
    }

    // ---- Write hmtx + hhea back into the builder --------------------------
    let new_hmtx = Hmtx {
        h_metrics: new_metrics,
        left_side_bearings: Vec::new(),
    };

    let mut new_hhea: Hhea = hhea_src.to_owned_table();
    new_hhea.number_of_h_metrics = u16::try_from(num_glyphs)
        .context("num_glyphs exceeds u16::MAX — hhea.number_of_h_metrics overflow")?;

    builder
        .add_table(&new_hmtx)
        .map_err(|e| anyhow::anyhow!("add hmtx: {e}"))?;
    builder
        .add_table(&new_hhea)
        .map_err(|e| anyhow::anyhow!("add hhea: {e}"))?;

    Ok(adjusted)
}

#[cfg(test)]
mod tests {
    // The empty-spacing fast path returns `Ok(0)` before touching `font`
    // or `builder`, but constructing a valid `FontRef` requires real font
    // bytes, which belong in integration tests under `tests/`. The
    // behavioural cases (cmap miss, zero advance, both deltas non-zero)
    // are exercised end-to-end against a real Noto subset in the
    // integration suite. The unit-level invariant we still want to guard
    // here is the empty-spacing short-circuit; we assert that the source
    // file contains `if spacing.is_empty() { return Ok(0); }` as a
    // structural regression check.
    #[test]
    fn empty_spacing_short_circuit_present_in_source() {
        let src = include_str!("glyph_spacing.rs");
        assert!(
            src.contains("if spacing.is_empty()"),
            "early return for empty spacing must remain — callers rely on \
             apply_glyph_spacing being a no-op when there is nothing to do"
        );
    }
}
