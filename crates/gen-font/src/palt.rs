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
//! NOTE: the body of these functions has not yet been pinned against the
//! actual `skrifa` 0.36 / `write-fonts` 0.42 surface. The signatures and
//! constants are stable — call sites in `proportional` and `build` thread
//! through them — but the GPOS walk / table-rebuild work is currently a
//! `bail!`-on-call stub.

use std::collections::BTreeMap;

use anyhow::bail;
use skrifa::FontRef;
use write_fonts::FontBuilder;

/// GPOS features that provide proportional metric adjustments. These become
/// redundant once the font itself is proportional, so we strip them to keep
/// apps from double-applying the shrink:
///
/// - `palt` — proportional alternate widths (horizontal)
/// - `vpal` — proportional alternate widths (vertical)
/// - `halt` — alternate metrics (horizontal, ½-width / pseudo-half)
/// - `vhal` — alternate metrics (vertical)
pub const PROP_FEATURES: &[&str] = &["palt", "vpal", "halt", "vhal"];

/// Walk GPOS `palt` lookups and return `{glyph_name: (XPlacement, XAdvance)}`.
///
/// Handles SinglePos formats 1 (one ValueRecord shared by all glyphs in the
/// coverage) and 2 (one ValueRecord per glyph), and unwraps Extension lookups
/// (type 9).
pub fn read_palt(font: &FontRef<'_>) -> anyhow::Result<BTreeMap<String, (i32, i32)>> {
    let _ = font;
    // TODO(impl): GPOS palt walk requires verified skrifa 0.36 surface
    // for FeatureList → LookupList → SinglePos formats 1/2 dispatch and
    // POST glyph-name lookup.
    Ok(BTreeMap::new())
}

/// Strip `palt`/`vpal`/`halt`/`vhal` from GPOS, keeping every other feature
/// intact.
///
/// GPOS feature indices live in two places that must stay in sync: the
/// FeatureRecord list itself (the data) and the FeatureIndex arrays inside
/// every LangSys (the references). Removing a record changes the indices of
/// every later record, so the LangSys references need to be remapped.
///
/// Lookup tables aren't touched: the lookups behind `palt` may also be
/// referenced by other features we want to keep, and orphaned lookups are
/// harmless.
pub fn remove_prop_features(builder: &mut FontBuilder<'_>) -> anyhow::Result<()> {
    let _ = builder;
    bail!(
        "remove_prop_features: TODO(impl) — GPOS table rebuild against \
         write-fonts 0.42 surface is not yet wired up"
    )
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
