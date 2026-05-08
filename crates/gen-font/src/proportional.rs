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
//!   `reduced_palt`. The `XPlacement` / `XAdvance` from palt is applied at full
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

use anyhow::{anyhow, bail, Context};
use read_fonts::types::Tag;
use skrifa::{raw::TableProvider, FontRef, GlyphId};
use write_fonts::{
    from_obj::{FromTableRef, ToOwnedTable},
    tables::{
        glyf::{GlyfLocaBuilder, Glyph},
        head::Head,
        hhea::Hhea,
        hmtx::{Hmtx, LongMetric},
        loca::LocaFormat,
    },
    FontBuilder,
};

/// Default scale for the reduced-palt bucket (e.g. punctuation): a third of
/// the full palt shift.
pub const DEFAULT_REDUCED_PALT_SCALE: f32 = 1.0 / 3.0;

/// Caller-tuneable knobs for [`make_proportional`].
///
/// All glyph-set fields are keyed by glyph id (gid) rather than glyph name.
/// This matches `read_palt`'s gid-keyed return and is robust against fonts
/// whose `post` table doesn't expose name strings (Noto Sans JP ships a
/// format-3 `post` whose names skrifa surfaces only as synthesised `gidNNN`
/// placeholders). Callers that want to drive these sets by codepoint should
/// resolve through the font's cmap before populating the options.
#[derive(Debug, Clone)]
pub struct ProportionalOptions {
    /// Glyph ids with palt entries that get a fraction of the adjustment
    /// (default 1/3) — used for punctuation.
    pub reduced_palt: Option<BTreeSet<u32>>,
    /// Scale applied to palt `XPlacement` / `XAdvance` for `reduced_palt` glyphs.
    pub reduced_palt_scale: f32,
    /// Glyph ids *without* palt entries that should still narrow proportionally:
    /// LSB and RSB each shrink by `1 - squeeze_sb_scale`.
    pub squeeze_sb: Option<BTreeSet<u32>>,
    /// Defaults to `reduced_palt_scale` when `None`.
    pub squeeze_sb_scale: Option<f32>,
    /// Caller-supplied palt table (gid → `(XPlacement, XAdvance)`) to use
    /// instead of reading from the font. Useful when variable-instantiation
    /// has corrupted the font's own palt — variable→static baking can leave
    /// palt `ValueRecords` with zeroed XPlacement/XAdvance pairs.
    pub palt_override: Option<BTreeMap<u32, (i32, i32)>>,
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
///
/// Reads the source `glyf` / `loca` / `hmtx` / `hhea` / `head` from `font`,
/// applies the palt-bake + squeeze-SB algorithm, and writes the rebuilt
/// `glyf` + `loca` + `head` (loca format) + `hmtx` + `hhea` back into
/// `builder` via [`FontBuilder::add_table`]. Same shape as
/// [`crate::strip_extreme::strip_extreme_glyphs`] — the read-side handle is
/// `font` and the write-side accumulator is `builder`.
pub fn make_proportional(
    font: &FontRef<'_>,
    builder: &mut FontBuilder<'_>,
    opts: &ProportionalOptions,
) -> anyhow::Result<()> {
    // ---- 1. Reject CFF outlines ----------------------------------------
    //
    // The Python reference checks `"glyf" not in font`. We mirror that by
    // bailing on fonts that ship CFF/CFF2 outlines — palt baking writes to
    // `glyf`, and the CFF compiler in write-fonts is out of scope here.
    if font.data_for_tag(Tag::new(b"CFF ")).is_some()
        || font.data_for_tag(Tag::new(b"CFF2")).is_some()
    {
        bail!("Only TrueType-outline fonts are supported");
    }
    if font.data_for_tag(Tag::new(b"glyf")).is_none() {
        bail!("Only TrueType-outline fonts are supported");
    }

    let squeeze_sb_scale = opts.squeeze_sb_scale.unwrap_or(opts.reduced_palt_scale);

    // ---- 2. Determine palt source --------------------------------------
    //
    // Caller override wins; otherwise read directly from GPOS palt. Mirrors
    // the Python reference (`palt_override if palt_override is not None
    // else _read_palt(font)`). Both paths return a gid-keyed map.
    let palt_adjustments: BTreeMap<u32, (i32, i32)> = match &opts.palt_override {
        Some(over) => over.clone(),
        None => crate::palt::read_palt(font)?,
    };

    // ---- 3. Read source tables -----------------------------------------
    let glyf = font.glyf().context("read glyf")?;
    let loca = font.loca(None).context("read loca")?;
    let maxp = font.maxp().context("read maxp")?;
    let num_glyphs = u32::from(maxp.num_glyphs());
    let hmtx_src = font.hmtx().context("read hmtx")?;
    let hhea_src = font.hhea().context("read hhea")?;
    let h_metrics = hmtx_src.h_metrics();
    let lsb_tail = hmtx_src.left_side_bearings();
    let num_long = usize::from(hhea_src.number_of_h_metrics());
    let trailing_advance = h_metrics.last().map_or(0, |m| m.advance());

    // Materialise the full long-metric array up front so per-gid mutations
    // can be applied in either pass. Same shape as
    // `tracking::apply_tracking` and `strip_extreme_glyphs`.
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
        new_metrics.push(LongMetric {
            advance,
            side_bearing,
        });
    }

    // Track glyph outline mutations by gid. We rebuild the entire `glyf`
    // table at the end (via `GlyfLocaBuilder`); for any gid not in this
    // map we round-trip the source glyph through `Glyph::from_table_ref`,
    // for gids in this map we emit the mutated value.
    let mut modified_glyphs: BTreeMap<u32, Glyph> = BTreeMap::new();

    // The reduced_palt set is keyed by gid (resolved by the caller — see
    // `ProportionalOptions` doc comment).
    let reduced_palt_gids: &BTreeSet<u32> = match &opts.reduced_palt {
        Some(s) => s,
        None => &EMPTY_GID_SET,
    };

    // ---- 4. Apply palt adjustments -------------------------------------
    //
    // For each (gid, (xp, xa)) in palt_adjustments:
    //   - if gid is in reduced_palt: scale (xp, xa) by reduced_palt_scale.
    //   - update hmtx[gid] = (aw + xa, lsb + xp).
    //   - if xp != 0 and the glyph has contours, shift its outline by xp.
    //
    // The `numberOfContours != 0` test in the Python reference excludes
    // truly-empty slots (loca says zero-length glyf bytes). Composite
    // glyphs report `-1` and are still shifted. write-fonts' `Glyph::Empty`
    // variant covers the "no outline" case.
    for (&gid, &(mut xp, mut xa)) in &palt_adjustments {
        if gid >= num_glyphs {
            // Defensive: a gid out of range. Skip silently to mirror the
            // Python's `if glyph_name not in hmtx.metrics`.
            continue;
        }

        if reduced_palt_gids.contains(&gid) {
            xp = ((xp as f32) * opts.reduced_palt_scale).round() as i32;
            xa = ((xa as f32) * opts.reduced_palt_scale).round() as i32;
        }

        let metric = &mut new_metrics[gid as usize];
        let aw = i32::from(metric.advance);
        let lsb = i32::from(metric.side_bearing);
        let new_lsb = lsb + xp;
        let new_aw = aw + xa;
        metric.advance = new_aw.clamp(0, i32::from(u16::MAX)) as u16;
        metric.side_bearing = new_lsb.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;

        // Shift the outline by xp if non-zero and the glyph actually has
        // contours. `loca.get_glyf` returns `Ok(None)` for empty slots.
        if xp != 0 {
            let gid_skrifa = GlyphId::new(gid);
            match loca.get_glyf(gid_skrifa, &glyf) {
                Ok(Some(read_g)) => {
                    if read_g.number_of_contours() != 0 {
                        let mut owned: Glyph = Glyph::from_table_ref(&read_g);
                        crate::glyph::shift_glyph_x(&mut owned, xp);
                        modified_glyphs.insert(gid, owned);
                    }
                }
                Ok(None) => {
                    // Empty slot; nothing to shift, hmtx already updated.
                }
                Err(e) => bail!("read glyf for gid {gid}: {e}"),
            }
        }
    }

    // ---- 5. Squeeze SB pass --------------------------------------------
    //
    // For each gid in opts.squeeze_sb (skip if also in palt_adjustments):
    //   - skip if out of range or numberOfContours == 0 (no outline).
    //   - bbox_w = xMax - xMin; rsb = aw - lsb - bbox_w.
    //   - cut = 1 - squeeze_sb_scale.
    //   - lsb_remove = round(lsb * cut); rsb_remove = round(rsb * cut).
    //   - if both are zero, skip.
    //   - shift outline left by -lsb_remove; new_lsb = lsb - lsb_remove;
    //     new_aw = aw - lsb_remove - rsb_remove.
    if let Some(squeeze_sb) = &opts.squeeze_sb {
        let cut = 1.0_f32 - squeeze_sb_scale;
        for &gid in squeeze_sb {
            // Skip if already handled in palt pass.
            if palt_adjustments.contains_key(&gid) {
                continue;
            }
            if gid >= num_glyphs {
                continue;
            }

            let read_glyph = match loca.get_glyf(GlyphId::new(gid), &glyf) {
                Ok(Some(g)) => g,
                Ok(None) => continue,
                Err(e) => bail!("read glyf for gid {gid}: {e}"),
            };
            if read_glyph.number_of_contours() == 0 {
                continue;
            }
            let x_min = i32::from(read_glyph.x_min());
            let x_max = i32::from(read_glyph.x_max());
            let bbox_w = x_max - x_min;

            let metric = &mut new_metrics[gid as usize];
            let aw = i32::from(metric.advance);
            let lsb = i32::from(metric.side_bearing);
            let rsb = aw - lsb - bbox_w;

            let lsb_remove = ((lsb as f32) * cut).round() as i32;
            let rsb_remove = ((rsb as f32) * cut).round() as i32;
            if lsb_remove == 0 && rsb_remove == 0 {
                continue;
            }

            // Shift outline left by lsb_remove (negative dx).
            let mut owned: Glyph = Glyph::from_table_ref(&read_glyph);
            crate::glyph::shift_glyph_x(&mut owned, -lsb_remove);
            modified_glyphs.insert(gid, owned);

            let new_lsb = lsb - lsb_remove;
            let new_aw = aw - lsb_remove - rsb_remove;
            metric.advance = new_aw.clamp(0, i32::from(u16::MAX)) as u16;
            metric.side_bearing = new_lsb.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        }
    }

    // ---- 6. Rebuild glyf + loca + head.indexToLocFormat ----------------
    //
    // Same pattern as `strip_extreme_glyphs`: walk gid 0..num_glyphs,
    // emit the mutated `Glyph` if present in `modified_glyphs`, otherwise
    // round-trip the source glyph (or `Glyph::Empty` for already-empty
    // slots). The slot index is preserved so downstream GSUB/GPOS
    // references stay correct.
    let mut glyph_builder = GlyfLocaBuilder::new();
    for gid in 0..num_glyphs {
        if let Some(modified) = modified_glyphs.remove(&gid) {
            glyph_builder
                .add_glyph(&modified)
                .map_err(|e| anyhow!("compile modified glyph at gid {gid}: {e}"))?;
            continue;
        }
        let gid_skrifa = GlyphId::new(gid);
        let glyph_typed: Glyph = match loca.get_glyf(gid_skrifa, &glyf) {
            Ok(Some(g)) => Glyph::from_table_ref(&g),
            Ok(None) => Glyph::Empty,
            Err(e) => bail!("read glyf for gid {gid}: {e}"),
        };
        glyph_builder
            .add_glyph(&glyph_typed)
            .map_err(|e| anyhow!("compile glyph at gid {gid}: {e}"))?;
    }
    let (new_glyf, new_loca, loca_format) = glyph_builder.build();

    // Update head.index_to_loc_format to match the new loca's format. The
    // GlyfLocaBuilder picks short vs long based on the largest offset, so
    // a font that grew past the 0xFFFF*2 short-format limit will roll over
    // to long here.
    let mut new_head: Head = font.head().context("read head")?.to_owned_table();
    new_head.index_to_loc_format = match loca_format {
        LocaFormat::Short => 0,
        LocaFormat::Long => 1,
    };

    // ---- 7. Rebuild hmtx + hhea ----------------------------------------
    //
    // Same shape as `tracking::apply_tracking`: emit a full
    // `num_glyphs`-long `LongMetric` array, empty `left_side_bearings`,
    // `hhea.number_of_h_metrics = num_glyphs`. Mirroring fontTools'
    // recompute on save — the heavy lifting (advance_width_max etc.) is
    // deferred to the end-of-Stage-2 metric recalc described in
    // `tracking.rs`.
    let new_hmtx = Hmtx {
        h_metrics: new_metrics,
        left_side_bearings: Vec::new(),
    };
    let mut new_hhea: Hhea = hhea_src.to_owned_table();
    new_hhea.number_of_h_metrics = u16::try_from(num_glyphs)
        .context("num_glyphs exceeds u16::MAX — hhea.number_of_h_metrics overflow")?;

    // ---- 8. Strip prop features ----------------------------------------
    //
    // Done before registering our rebuilt tables on the builder so the
    // GPOS rewrite can't be clobbered by a later override (it isn't —
    // `add_table` replaces by tag — but the explicit ordering matches the
    // Python reference, which strips features at the end of
    // `make_proportional`).
    crate::palt::remove_prop_features(font, builder)?;

    // ---- 9. Register rebuilt tables ------------------------------------
    builder
        .add_table(&new_glyf)
        .map_err(|e| anyhow!("add glyf: {e}"))?;
    builder
        .add_table(&new_loca)
        .map_err(|e| anyhow!("add loca: {e}"))?;
    builder
        .add_table(&new_head)
        .map_err(|e| anyhow!("add head: {e}"))?;
    builder
        .add_table(&new_hmtx)
        .map_err(|e| anyhow!("add hmtx: {e}"))?;
    builder
        .add_table(&new_hhea)
        .map_err(|e| anyhow!("add hhea: {e}"))?;

    // TODO(impl): the Python reference's `_apply_glyph_spacing` /
    // `apply_tracking` and friends recompute `hhea.advance_width_max`,
    // `hhea.min_left_side_bearing`, `hhea.min_right_side_bearing`, and
    // `hhea.x_max_extent` after every metrics-touching pass; mirroring
    // that here would require a full glyf walk. The single recalc at the
    // end of Stage 2 is the right place — see the matching note in
    // `tracking.rs`.

    Ok(())
}

/// Sentinel empty set, used as a fallback when `opts.reduced_palt` is `None`.
/// Avoids allocating an empty `BTreeSet` per call.
static EMPTY_GID_SET: BTreeSet<u32> = BTreeSet::new();

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
