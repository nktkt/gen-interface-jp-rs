//! GPOS `palt` extraction and proportional-feature stripping.
//!
//! Ports `_read_palt`, `_remove_prop_features`, `_filter_feature_indices`,
//! `_remap_feature_indices`, and the `PROP_FEATURES` constant from
//! `source/src/font/proportional.py`.
//!
//! - **`read_palt`** walks the GPOS `palt` feature and harvests
//!   `(XPlacement, XAdvance)` tuples per glyph so callers can bake those
//!   adjustments into `hmtx`.
//! - **`remove_prop_features`** strips `palt` / `vpal` / `halt` / `vhal` from
//!   GPOS so apps that honour those features don't double-apply the shrink we
//!   just baked in.
//!
//! NOTE: `read_palt` is implemented against the typed `read-fonts` 0.34 GPOS
//! reader (re-exported through skrifa 0.36). `remove_prop_features` rebuilds
//! the GPOS `FeatureList` + `ScriptList` using the `write-fonts` 0.42 owned types
//! and registers the result on the supplied [`FontBuilder`] via
//! [`FontBuilder::add_table`] — same shape as [`crate::strip_extreme`].

use std::collections::{BTreeMap, BTreeSet};

use anyhow::anyhow;
use read_fonts::{
    tables::gpos::{ExtensionSubtable, PositionLookup, SinglePos},
    types::Tag,
    TableProvider,
};
use skrifa::FontRef;
use write_fonts::{from_obj::ToOwnedTable, tables::gpos::Gpos, FontBuilder};

/// GPOS features that provide proportional metric adjustments. These become
/// redundant once the font itself is proportional, so we strip them to keep
/// apps from double-applying the shrink:
///
/// - `palt` — proportional alternate widths (horizontal)
/// - `vpal` — proportional alternate widths (vertical)
/// - `halt` — alternate metrics (horizontal, ½-width / pseudo-half)
/// - `vhal` — alternate metrics (vertical)
pub const PROP_FEATURES: &[&str] = &["palt", "vpal", "halt", "vhal"];

/// Walk GPOS `palt` lookups and return `{glyph_id: (XPlacement, XAdvance)}`.
///
/// Handles `SinglePos` formats 1 (one `ValueRecord` shared by all glyphs in the
/// coverage) and 2 (one `ValueRecord` per glyph), and unwraps Extension lookups
/// (type 9).
///
/// Keying by glyph id (rather than glyph name) is robust against fonts whose
/// `post` table doesn't expose name strings — Noto Sans JP ships a format-3
/// `post` table whose names skrifa surfaces only as synthesised `gidNNN`
/// placeholders. Downstream consumers (`make_proportional`,
/// `apply_tracking`) look glyphs up by id anyway, so name-keying buys
/// nothing and loses information when the font omits names.
pub fn read_palt(font: &FontRef<'_>) -> anyhow::Result<BTreeMap<u32, (i32, i32)>> {
    // Missing GPOS / FeatureList / no `palt` records all surface as "no
    // adjustments". Callers treat the absence of palt as "leave hmtx alone",
    // so we mirror the Python behaviour of returning an empty map rather
    // than erroring.
    let Ok(gpos) = font.gpos() else {
        return Ok(BTreeMap::new());
    };
    let Ok(feature_list) = gpos.feature_list() else {
        return Ok(BTreeMap::new());
    };
    let Ok(lookup_list) = gpos.lookup_list() else {
        return Ok(BTreeMap::new());
    };

    // Find every `palt` FeatureRecord and accumulate the lookup indices it
    // points at. Real fonts only have one, but the spec allows any number.
    let palt_tag = Tag::new(b"palt");
    let feature_data = feature_list.offset_data();
    let mut palt_lookup_indices: Vec<u16> = Vec::new();
    for record in feature_list.feature_records() {
        if record.feature_tag() != palt_tag {
            continue;
        }
        let Ok(feature) = record.feature(feature_data) else {
            continue;
        };
        for raw in feature.lookup_list_indices() {
            palt_lookup_indices.push(raw.get());
        }
    }
    if palt_lookup_indices.is_empty() {
        return Ok(BTreeMap::new());
    }

    let lookups = lookup_list.lookups();
    let mut adjustments: BTreeMap<u32, (i32, i32)> = BTreeMap::new();
    for index in palt_lookup_indices {
        let Ok(lookup) = lookups.get(index as usize) else {
            continue;
        };
        match lookup {
            PositionLookup::Single(single_lookup) => {
                for subtable in single_lookup.subtables().iter().flatten() {
                    record_single_pos(&subtable, &mut adjustments);
                }
            }
            PositionLookup::Extension(extension_lookup) => {
                for ext in extension_lookup.subtables().iter().flatten() {
                    // The Python port only cares about palt-style SinglePos
                    // adjustments (XPlacement / XAdvance per glyph), so we
                    // unwrap Extension lookups whose inner subtable is
                    // SinglePos and ignore the rest in line with
                    // `_read_palt`.
                    if let ExtensionSubtable::Single(ext_single) = ext {
                        if let Ok(inner) = ext_single.extension() {
                            record_single_pos(&inner, &mut adjustments);
                        }
                    }
                }
            }
            // PairPos / contextual / mark lookups don't appear in real-world
            // palt features. The Python reference skips them silently and we
            // do the same.
            _ => {}
        }
    }

    Ok(adjustments)
}

/// Visit a `SinglePos` subtable and record `(XPlacement, XAdvance)` for every
/// glyph it covers, mirroring the Python `_read_palt` inner loop.
fn record_single_pos(subtable: &SinglePos<'_>, adjustments: &mut BTreeMap<u32, (i32, i32)>) {
    match subtable {
        SinglePos::Format1(fmt1) => {
            // Format 1: one ValueRecord shared by every glyph in the
            // coverage table.
            let Ok(coverage) = fmt1.coverage() else {
                return;
            };
            let value = fmt1.value_record();
            let xp = i32::from(value.x_placement().unwrap_or(0));
            let xa = i32::from(value.x_advance().unwrap_or(0));
            for gid16 in coverage.iter() {
                adjustments.insert(u32::from(gid16.to_u16()), (xp, xa));
            }
        }
        SinglePos::Format2(fmt2) => {
            // Format 2: one ValueRecord per glyph, indexed in coverage
            // order.
            let Ok(coverage) = fmt2.coverage() else {
                return;
            };
            let values = fmt2.value_records();
            for (idx, gid16) in coverage.iter().enumerate() {
                let Ok(value) = values.get(idx) else {
                    continue;
                };
                let xp = i32::from(value.x_placement().unwrap_or(0));
                let xa = i32::from(value.x_advance().unwrap_or(0));
                adjustments.insert(u32::from(gid16.to_u16()), (xp, xa));
            }
        }
    }
}

/// Strip `palt`/`vpal`/`halt`/`vhal` from GPOS, keeping every other feature
/// intact.
///
/// GPOS feature indices live in two places that must stay in sync: the
/// `FeatureRecord` list itself (the data) and the `FeatureIndex` arrays inside
/// every `LangSys` (the references). Removing a record changes the indices of
/// every later record, so the `LangSys` references need to be remapped.
///
/// Lookup tables aren't touched: the lookups behind `palt` may also be
/// referenced by other features we want to keep, and orphaned lookups are
/// harmless.
pub fn remove_prop_features(
    font: &FontRef<'_>,
    builder: &mut FontBuilder<'_>,
) -> anyhow::Result<()> {
    // Step 1: read the source GPOS. Missing GPOS / FeatureList both surface as
    // "nothing to strip", matching the Python reference's early-return.
    let Ok(gpos_src) = font.gpos() else {
        return Ok(());
    };
    let Ok(feature_list_src) = gpos_src.feature_list() else {
        return Ok(());
    };

    // Step 2: walk FeatureRecords and collect the indices of palt/vpal/halt/vhal.
    // We compare by tag bytes (Tag::new) rather than allocating per-record
    // strings.
    // PROP_FEATURES is a static table of 4-byte ASCII tags. `new_checked`
    // fails iff the byte slice isn't 4 bytes — which would be a programmer
    // error in PROP_FEATURES, so surface it as anyhow rather than panic.
    let mut prop_tags: BTreeSet<Tag> = BTreeSet::new();
    for t in PROP_FEATURES {
        let tag = Tag::new_checked(t.as_bytes())
            .map_err(|e| anyhow!("PROP_FEATURES contains invalid tag {t:?}: {e}"))?;
        prop_tags.insert(tag);
    }

    let mut to_remove: BTreeSet<usize> = BTreeSet::new();
    for (i, record) in feature_list_src.feature_records().iter().enumerate() {
        if prop_tags.contains(&record.feature_tag()) {
            to_remove.insert(i);
        }
    }

    // Step 3: nothing to do — leave the GPOS bytes from the source untouched.
    // The caller's `FontBuilder` will pick them up via `copy_missing_tables`.
    if to_remove.is_empty() {
        return Ok(());
    }

    // Step 4: load the whole GPOS as an owned write-fonts struct so we can
    // mutate FeatureList + ScriptList in place. The lookup list is left alone
    // — palt-backing lookups may be referenced by other features we keep, and
    // orphaned lookups are harmless (matches the Python comment).
    let mut gpos: Gpos = gpos_src.to_owned_table();

    let n = gpos.feature_list.feature_records.len();

    // Build the (old → new) remap for kept indices: kept are the sorted
    // 0..n excluding to_remove; remap[old] = position-within-kept.
    let kept: Vec<usize> = (0..n).filter(|i| !to_remove.contains(i)).collect();
    let mut remap: BTreeMap<u16, u16> = BTreeMap::new();
    for (new_idx, &old_idx) in kept.iter().enumerate() {
        // Both fit in u16 because FeatureList is u16-counted (same invariant
        // FontWrite::write_into asserts via `u16::try_from`).
        let old_u16 = u16::try_from(old_idx)
            .map_err(|_| anyhow!("feature index {old_idx} exceeds u16::MAX"))?;
        let new_u16 = u16::try_from(new_idx)
            .map_err(|_| anyhow!("feature index {new_idx} exceeds u16::MAX"))?;
        remap.insert(old_u16, new_u16);
    }

    // Step 5: rebuild FeatureRecord list with only the kept records, in
    // original order. We pull them out by index from the existing Vec; the
    // new index is `kept.iter().position(|&k| k == old_idx)`.
    let mut new_records = Vec::with_capacity(kept.len());
    // Drain in reverse so we can `swap_remove` cheaply? Actually with a Vec,
    // simpler and still O(n) is to consume by `mem::take` + re-collect:
    let original_records = std::mem::take(&mut gpos.feature_list.feature_records);
    for (i, record) in original_records.into_iter().enumerate() {
        if !to_remove.contains(&i) {
            new_records.push(record);
        }
    }
    gpos.feature_list.feature_records = new_records;

    // Step 6: walk every Script's DefaultLangSys + LangSysRecords and rewrite
    // each LangSys.feature_indices: drop indices in to_remove, then remap the
    // survivors. Mirrors `_filter_feature_indices` followed by
    // `_remap_feature_indices` from the Python reference.
    let script_list = &mut *gpos.script_list;
    for script_record in &mut script_list.script_records {
        let script = &mut *script_record.script;

        if let Some(default_langsys) = script.default_lang_sys.as_mut() {
            rewrite_feature_indices(&mut default_langsys.feature_indices, &remap);
            // Spec: required_feature_index = 0xFFFF means "no required feature".
            // If the required feature pointed at one of the removed records,
            // drop the requirement; otherwise remap it.
            if default_langsys.required_feature_index != 0xFFFF {
                default_langsys.required_feature_index = remap
                    .get(&default_langsys.required_feature_index)
                    .copied()
                    .unwrap_or(0xFFFF);
            }
        }

        for langsys_record in &mut script.lang_sys_records {
            let langsys = &mut *langsys_record.lang_sys;
            rewrite_feature_indices(&mut langsys.feature_indices, &remap);
            if langsys.required_feature_index != 0xFFFF {
                langsys.required_feature_index = remap
                    .get(&langsys.required_feature_index)
                    .copied()
                    .unwrap_or(0xFFFF);
            }
        }
    }

    // Step 7: register the rebuilt GPOS on the builder. Same shape as
    // `strip_extreme_glyphs` — `add_table` overrides any earlier entry for
    // the same tag, so seeding the builder from the source bytes via
    // `copy_missing_tables` afterwards is safe.
    builder
        .add_table(&gpos)
        .map_err(|e| anyhow!("add GPOS: {e}"))?;

    Ok(())
}

/// Filter and remap a `LangSys.feature_indices` array in place. Mirrors
/// `_filter_feature_indices` + `_remap_feature_indices` from the Python
/// reference fused together: drop entries whose old index is in `to_remove`
/// (i.e. not present in `remap`), then translate survivors.
fn rewrite_feature_indices(indices: &mut Vec<u16>, remap: &BTreeMap<u16, u16>) {
    indices.retain(|i| remap.contains_key(i));
    for i in indices.iter_mut() {
        // `retain` above guarantees the lookup hits.
        if let Some(&new_i) = remap.get(i) {
            *i = new_i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn prop_features_contains_exactly_the_four_tags() {
        let s: HashSet<&str> = PROP_FEATURES.iter().copied().collect();
        let expected: HashSet<&str> = ["palt", "vpal", "halt", "vhal"].into_iter().collect();
        assert_eq!(s, expected);
        assert_eq!(PROP_FEATURES.len(), 4);
    }
}
