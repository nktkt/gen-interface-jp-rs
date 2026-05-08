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

use std::collections::BTreeSet;

use anyhow::{anyhow, bail, Context};
use read_fonts::types::GlyphId16;
use skrifa::{raw::TableProvider, FontRef, MetadataProvider};
use write_fonts::{
    from_obj::{FromTableRef, ToOwnedTable},
    tables::{
        cmap::Cmap,
        glyf::{GlyfLocaBuilder, Glyph},
        gsub::{Gsub, SingleSubst, SubstitutionLookup},
        head::Head,
        hmtx::{Hmtx, LongMetric},
        layout::CoverageTable,
        loca::LocaFormat,
    },
    FontBuilder,
};

/// Threshold for "extreme" glyphs whose bbox dominates head.yMax/yMin.
///
/// em=1000 base; the legitimate Latin/CJK content of Noto stays well within
/// these bounds, so anything past them is the vertical-only iteration-mark
/// glyphs we want to neutralise.
pub const EXTREME_YMAX: i32 = 1200;
pub const EXTREME_YMIN: i32 = -400;

/// Neutralise glyphs whose bbox extends far beyond the em-square. Returns the
/// number of glyphs that were neutralised.
///
/// Reads the source `glyf` / `loca` / `hmtx` / `cmap` / `GSUB` from `font`,
/// identifies glyphs whose bbox extends past [`EXTREME_YMAX`] / [`EXTREME_YMIN`],
/// and writes the neutralised tables back into `builder` via
/// [`FontBuilder::add_table`]. Mirrors `tracking::apply_tracking` in shape:
/// the read-side handle is `font`, the write-side accumulator is `builder`,
/// and the two are kept in sync by the caller (the builder is seeded from the
/// same bytes `font` parses, so any tags we add here override previous
/// entries in the builder).
///
/// What this does, in order:
///
/// 1. Walk `glyf` / `loca`. Collect every gid whose `numberOfContours != 0`
///    AND (`yMax > EXTREME_YMAX` OR `yMin < EXTREME_YMIN`).
/// 2. If that set is empty, return 0 without touching the builder.
/// 3. Rebuild `glyf` + `loca` + `head.indexToLocFormat`: target gids become
///    [`Glyph::Empty`], every other slot is copied through verbatim from the
///    source bytes. The slot index is preserved so downstream GSUB / GPOS
///    references stay correct.
/// 4. Rebuild `hmtx`: target gids get `(0, 0)`. Every other glyph keeps its
///    original `(advance, lsb)`. We materialise a full `num_glyphs`-long
///    long-metric array (matching `tracking::apply_tracking`) so the result
///    is robust against later passes that assume a long-metric per glyph.
/// 5. Rebuild `cmap`: drop every `(codepoint, gid)` entry whose gid is in
///    the target set, then re-emit a fresh format-4 + format-12 cmap via
///    [`Cmap::from_mappings`]. Codepoints with neutralised glyphs fall
///    through to `.notdef` when typed.
/// 6. Rebuild `GSUB` (best-effort): walk every Single-substitution lookup,
///    including those wrapped in an Extension subtable, and drop coverage
///    entries whose key OR value glyph is in the target set. Other GSUB
///    lookup types are left untouched — the Python reference filters
///    `st.mapping` per-subtable, which `fontTools` only attaches to
///    Single-subst.
pub fn strip_extreme_glyphs(
    font: &FontRef<'_>,
    builder: &mut FontBuilder<'_>,
) -> anyhow::Result<usize> {
    // ---- 1. Identify target glyphs --------------------------------------
    let glyf = font.glyf().context("read glyf")?;
    let loca = font.loca(None).context("read loca")?;
    let maxp = font.maxp().context("read maxp")?;
    let num_glyphs = u32::from(maxp.num_glyphs());

    let mut targets: BTreeSet<u32> = BTreeSet::new();
    for gid in 0..num_glyphs {
        // Empty / out-of-range slot — has no outline so no bbox to
        // dominate `head`. Skip silently, matching the Python reference
        // which iterates `font.getGlyphOrder()` and does the same
        // `numberOfContours == 0` early-return.
        let Ok(Some(glyph)) = loca.get_glyf(skrifa::GlyphId::new(gid), &glyf) else {
            continue;
        };
        if glyph.number_of_contours() == 0 {
            continue;
        }
        let y_max = i32::from(glyph.y_max());
        let y_min = i32::from(glyph.y_min());
        if y_max > EXTREME_YMAX || y_min < EXTREME_YMIN {
            targets.insert(gid);
        }
    }

    if targets.is_empty() {
        return Ok(0);
    }
    let count = targets.len();

    // ---- 2. Rebuild glyf + loca (+ head.indexToLocFormat) ---------------
    //
    // We can't mutate the existing `glyf`/`loca` in place because their
    // structure is offset-encoded and editing one cell requires recomputing
    // every offset. Instead we build a fresh pair via `GlyfLocaBuilder`,
    // emitting `Glyph::Empty` for target gids and copying the original
    // bytes verbatim for everything else. Slot positions are preserved so
    // GSUB / GPOS index references stay valid.
    let mut glyph_builder = GlyfLocaBuilder::new();
    for gid in 0..num_glyphs {
        let gid_skrifa = skrifa::GlyphId::new(gid);
        if targets.contains(&gid) {
            // Empty glyph — numberOfContours = 0, no bytes written into
            // `glyf`, just an entry in `loca`. Bbox vanishes from `head`.
            glyph_builder
                .add_glyph(&Glyph::Empty)
                .map_err(|e| anyhow!("compile empty glyph at gid {gid}: {e}"))?;
            continue;
        }
        // Non-target glyph: re-parse and re-emit through the typed write
        // path. We can't byte-copy because `GlyfLocaBuilder::add_glyph`
        // takes a typed `Glyph`, but the round-trip is faithful: write-fonts
        // implements `FromObjRef` for both Simple and Composite glyphs.
        let glyph_typed: Glyph = match loca.get_glyf(gid_skrifa, &glyf) {
            Ok(Some(g)) => Glyph::from_table_ref(&g),
            // Already-empty slot — just push another Empty.
            Ok(None) => Glyph::Empty,
            Err(e) => bail!("read glyf for gid {gid}: {e}"),
        };
        glyph_builder
            .add_glyph(&glyph_typed)
            .map_err(|e| anyhow!("compile glyph at gid {gid}: {e}"))?;
    }
    let (new_glyf, new_loca, loca_format) = glyph_builder.build();

    // The new loca format may differ from the source's — `GlyfLocaBuilder`
    // picks short vs long based on the largest offset. Update `head` to
    // match so the consumer reads loca with the right element width.
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

    // ---- 3. Rebuild hmtx ------------------------------------------------
    //
    // Same shape as `tracking::apply_tracking`: materialise a full
    // `num_glyphs`-long `LongMetric` array, mutate target gids to `(0, 0)`,
    // emit with empty `left_side_bearings`. This trades a few hundred bytes
    // of hmtx growth for a robust shape that survives later metric passes.
    let hmtx_src = font.hmtx().context("read hmtx")?;
    let hhea_src = font.hhea().context("read hhea")?;
    let h_metrics = hmtx_src.h_metrics();
    let lsb_tail = hmtx_src.left_side_bearings();
    let num_long = usize::from(hhea_src.number_of_h_metrics());
    let trailing_advance = h_metrics.last().map_or(0, |m| m.advance());

    let mut new_metrics: Vec<LongMetric> = Vec::with_capacity(num_glyphs as usize);
    for gid in 0..num_glyphs {
        if targets.contains(&gid) {
            new_metrics.push(LongMetric {
                advance: 0,
                side_bearing: 0,
            });
            continue;
        }
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
    let new_hmtx = Hmtx {
        h_metrics: new_metrics,
        left_side_bearings: Vec::new(),
    };
    let mut new_hhea: write_fonts::tables::hhea::Hhea = hhea_src.to_owned_table();
    new_hhea.number_of_h_metrics = u16::try_from(num_glyphs)
        .context("num_glyphs exceeds u16::MAX — hhea.number_of_h_metrics overflow")?;
    builder
        .add_table(&new_hmtx)
        .map_err(|e| anyhow!("add hmtx: {e}"))?;
    builder
        .add_table(&new_hhea)
        .map_err(|e| anyhow!("add hhea: {e}"))?;

    // ---- 4. Rebuild cmap ------------------------------------------------
    //
    // `Cmap::from_mappings` consumes a sorted/dedup'd `(char, GlyphId)`
    // iterator and emits a canonical (Unicode + Windows) (BMP + full) set
    // of subtables. Drop every entry whose gid is in the target set.
    //
    // We use skrifa's `charmap().mappings()`, which picks the most-suitable
    // codepoint subtable (format 4 / 12) and yields `(u32, GlyphId)`. The
    // Python reference filters every subtable in `font["cmap"].tables`
    // independently; in practice Noto Sans JP only carries the canonical
    // pair, and `Cmap::from_mappings` regenerates that pair byte-for-byte
    // from the kept mappings.
    let charmap = font.charmap();
    let kept_mappings = charmap.mappings().filter_map(|(cp, gid)| {
        if targets.contains(&gid.to_u32()) {
            return None;
        }
        let ch = char::from_u32(cp)?;
        Some((ch, gid))
    });
    let new_cmap = Cmap::from_mappings(kept_mappings).map_err(|e| anyhow!("rebuild cmap: {e}"))?;
    builder
        .add_table(&new_cmap)
        .map_err(|e| anyhow!("add cmap: {e}"))?;

    // ---- 5. Rebuild GSUB (best-effort, single-substitution only) -------
    //
    // The Python reference filters `st.mapping` on every subtable that has
    // one, which `fontTools` only attaches to Single-substitution tables.
    // We mirror that: walk every `SubstitutionLookup::Single` (and the
    // single-subst case inside `SubstitutionLookup::Extension`) and drop
    // coverage entries whose key OR value glyph is in the target set.
    //
    // Format-1 (delta) lookups are converted to format-2 (explicit list)
    // when filtering would split them across delta groups — losing the
    // single delta but preserving correctness.
    if let Ok(gsub) = font.gsub() {
        let mut new_gsub: Gsub = gsub.to_owned_table();
        let target_u32: BTreeSet<u32> = targets.iter().copied().collect();
        let lookup_list = &mut *new_gsub.lookup_list;
        for lookup_marker in &mut lookup_list.lookups {
            let lookup = &mut **lookup_marker;
            match lookup {
                SubstitutionLookup::Single(single_lookup) => {
                    for sub in &mut single_lookup.subtables {
                        prune_single_subst(sub, &target_u32);
                    }
                }
                SubstitutionLookup::Extension(ext_lookup) => {
                    for ext in &mut ext_lookup.subtables {
                        if let write_fonts::tables::gsub::ExtensionSubtable::Single(ext_single) =
                            &mut **ext
                        {
                            prune_single_subst(&mut ext_single.extension, &target_u32);
                        }
                    }
                }
                _ => {
                    // Non-single-subst lookups: the Python reference doesn't
                    // touch these (no `st.mapping` attribute), and an honest
                    // port of the Python's *additional* coverage cleanup for
                    // multi-subst / ligature / contextual lookups is
                    // out of scope for this pass.
                }
            }
        }
        builder
            .add_table(&new_gsub)
            .map_err(|e| anyhow!("add GSUB: {e}"))?;
    }

    Ok(count)
}

/// Strip every (key, value) pair from a Single-substitution subtable where
/// either side is in the target set.
///
/// Format-1 (single delta) is rewritten as Format-2 (explicit substitute
/// list) so partial coverage filtering doesn't have to preserve the delta
/// invariant. The output is byte-larger by `2 * coverage_len - 2` bytes,
/// which is negligible for the handful of palt/vert/vrt2 lookups we touch.
fn prune_single_subst(subst: &mut SingleSubst, targets: &BTreeSet<u32>) {
    // Resolve the original (key, value) pairs from whichever format the
    // subtable currently uses, then filter and re-emit.
    let pairs: Vec<(GlyphId16, GlyphId16)> = match subst {
        SingleSubst::Format1(fmt1) => {
            let delta = fmt1.delta_glyph_id;
            coverage_glyphs(&fmt1.coverage)
                .map(|gid| {
                    // Spec: substitute = (key + delta) mod 65536.
                    let key = gid.to_u16();
                    let sub = key.wrapping_add(delta as u16);
                    (gid, GlyphId16::new(sub))
                })
                .collect()
        }
        SingleSubst::Format2(fmt2) => {
            let coverage_glyphs: Vec<GlyphId16> = coverage_glyphs(&fmt2.coverage).collect();
            coverage_glyphs
                .into_iter()
                .zip(fmt2.substitute_glyph_ids.iter().copied())
                .collect()
        }
    };

    let kept: Vec<(GlyphId16, GlyphId16)> = pairs
        .into_iter()
        .filter(|(k, v)| {
            !targets.contains(&u32::from(k.to_u16())) && !targets.contains(&u32::from(v.to_u16()))
        })
        .collect();

    if kept.is_empty() {
        // Empty coverage — collapse to an explicit empty Format2. We can't
        // actually delete the subtable from the lookup without renumbering
        // every later lookup index in GPOS / GSUB ContextualLookup chains,
        // mirroring the Python's choice to leave the (now harmless) shell.
        *subst = SingleSubst::format_2(CoverageTable::format_1(Vec::new()), Vec::new());
        return;
    }

    // Re-emit as Format-2 unconditionally. The format-1 (delta) shape is an
    // optimisation for densely-packed contiguous coverage with a uniform
    // delta; once we've filtered we can't guarantee that, and the Format-2
    // bytes-on-disk overhead is bounded.
    let coverage_glyphs: Vec<GlyphId16> = kept.iter().map(|(k, _)| *k).collect();
    let substitutes: Vec<GlyphId16> = kept.iter().map(|(_, v)| *v).collect();
    *subst = SingleSubst::format_2(CoverageTable::format_1(coverage_glyphs), substitutes);
}

/// Iterate every glyph id covered by a write-fonts `CoverageTable`, in
/// coverage-index order (matching the OpenType-spec semantics for
/// `SingleSubstFormat2`'s substitute-array indexing).
fn coverage_glyphs<'a>(coverage: &'a CoverageTable) -> Box<dyn Iterator<Item = GlyphId16> + 'a> {
    match coverage {
        CoverageTable::Format1(fmt1) => Box::new(fmt1.glyph_array.iter().copied()),
        CoverageTable::Format2(fmt2) => Box::new(fmt2.range_records.iter().flat_map(|rr| {
            let start = rr.start_glyph_id.to_u16();
            let end = rr.end_glyph_id.to_u16();
            (start..=end).map(GlyphId16::new)
        })),
    }
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
