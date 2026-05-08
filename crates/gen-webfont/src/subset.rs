//! WOFF2 subsetting helpers.
//!
//! Ported from `source/src/webfont/build.py` (`_subset_options`,
//! `build_woff2_subset`, `build_full_woff2`).
//!
//! The Python reference relies on fontTools' `subset` module, which has no
//! direct Rust equivalent. We sketch the algorithm using `skrifa` for parsing
//! the source TTF and `write-fonts` (a.k.a. `build-fonts`) for emitting a new
//! TTF, then convert that TTF to WOFF2 via the `woff2` crate. The heavy
//! lifting (composite-glyph closure, GSUB closure, table rewriting) is left
//! marked with `// TODO(impl):` until the full subsetter is wired up.
//!
//! fontTools subsetter options used by the Python reference, for record:
//!
//! ```text
//!   flavor          = "woff2"
//!   retain_gids     = false
//!   glyph_names     = false
//!   layout_features = ["*"]            // keep every layout feature
//!   name_IDs        = [1, 2, 3, 4, 5, 6, 16, 17]
//!   name_legacy     = false
//!   name_languages  = ["*"]
//!   drop_tables     = ["DSIG"]
//! ```

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};

/// Name table IDs the Python reference retains (`options.name_IDs`).
///
/// 1=Family, 2=Subfamily, 3=Unique ID, 4=Full name, 5=Version, 6=PostScript,
/// 16=Typographic family, 17=Typographic subfamily.
const KEEP_NAME_IDS: &[u16] = &[1, 2, 3, 4, 5, 6, 16, 17];

/// Tables the Python reference unconditionally drops (`options.drop_tables`).
const DROP_TABLES: &[&[u8; 4]] = &[b"DSIG"];

/// Internal options struct mirroring the fontTools `subset.Options` knobs
/// that matter to us. Kept private; callers go through the two public
/// helpers below.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SubsetOptions {
    /// `flavor = "woff2"` — output container.
    flavor_woff2: bool,
    /// `retain_gids = False` — renumber glyph IDs densely from 0.
    retain_gids: bool,
    /// `glyph_names = False` — strip post-table glyph names.
    glyph_names: bool,
    /// `layout_features = ["*"]` — keep every GSUB/GPOS feature.
    keep_all_layout_features: bool,
    /// `name_IDs = [1, 2, 3, 4, 5, 6, 16, 17]` — name records to retain.
    keep_name_ids: Vec<u16>,
    /// `name_legacy = False` — drop legacy (Mac, etc.) name records.
    name_legacy: bool,
    /// `name_languages = ["*"]` — keep all languages of the retained IDs.
    keep_all_name_languages: bool,
    /// `drop_tables = ["DSIG"]` — explicit table drop list.
    drop_tables: Vec<[u8; 4]>,
}

impl SubsetOptions {
    /// Equivalent of Python `_subset_options()`.
    fn for_woff2_subset() -> Self {
        Self {
            flavor_woff2: true,
            retain_gids: false,
            glyph_names: false,
            keep_all_layout_features: true,
            keep_name_ids: KEEP_NAME_IDS.to_vec(),
            name_legacy: false,
            keep_all_name_languages: true,
            drop_tables: DROP_TABLES.iter().map(|t| **t).collect(),
        }
    }
}

/// Subset a TTF down to the given codepoints and emit a WOFF2 file at
/// `out_path`.
///
/// Mirrors `build_woff2_subset` in `source/src/webfont/build.py`.
pub fn build_woff2_subset(src_ttf: &Path, _out_path: &Path, codepoints: &[u32]) -> Result<()> {
    let options = SubsetOptions::for_woff2_subset();

    // Deduplicate + sort, matching Python's `sorted(set(codepoints))`.
    let unicodes: BTreeSet<u32> = codepoints.iter().copied().collect();

    let src_bytes = std::fs::read(src_ttf)
        .with_context(|| format!("reading source TTF {}", src_ttf.display()))?;

    // Step 1: parse the source font.
    //
    // TODO(impl): wire up `skrifa::FontRef::new(&src_bytes)` and pull out the
    // cmap / glyf / GSUB / GPOS / name tables we need below. For now we just
    // hold the raw bytes so the rest of the sketch type-checks.
    let _ = &src_bytes;

    // Step 2: determine the glyph IDs to keep.
    //
    // 2a. cmap walk: every gid that any of `unicodes` maps to.
    // 2b. composite-glyph closure: for each retained gid in `glyf`, recursively
    //     pull in the gids referenced by its component records.
    // 2c. GSUB closure: substitutions reachable from the retained features
    //     (Python uses `layout_features = ["*"]`, so every feature counts).
    //     This must run to a fixed point — alternates of alternates show up
    //     in real fonts (e.g. ligature → small-cap → swash chains).
    //
    // TODO(impl): implement the closure. For the sketch we just collect what
    // we can from a direct cmap walk.
    let mut keep_gids: BTreeSet<u32> = BTreeSet::new();
    keep_gids.insert(0); // .notdef must always be retained.

    // TODO(impl): walk cmap subtables here and extend `keep_gids` with the
    // glyphs reachable from `unicodes`. Then run composite + GSUB closure.
    let _ = &unicodes;

    // Step 3: build the new font.
    //
    // - Renumber the retained gids densely from 0 (because
    //   `options.retain_gids == false`).
    // - Rewrite cmap to only contain the retained codepoints.
    // - Keep every layout feature (`keep_all_layout_features`) but prune the
    //   lookups so they only reference retained gids.
    // - Filter `name` table down to `keep_name_ids`, dropping legacy records
    //   and keeping all languages of the retained IDs.
    // - Strip glyph names from `post` (set format 3.0).
    //
    // TODO(impl): emit via `write-fonts` (build-fonts). Until that is wired
    // up, we can't actually produce a TTF, so we surface the missing
    // implementation as an error rather than silently writing garbage.
    let _ = &options;
    let _new_ttf_bytes: Vec<u8> = {
        // TODO(impl): replace this with the real builder output.
        return Err(anyhow::anyhow!(
            "build_woff2_subset: subsetter not yet implemented \
             (skrifa parse + write-fonts emit pending)"
        ));
    };

    // Step 4: drop DSIG (and anything else in `options.drop_tables`).
    //
    // TODO(impl): in practice this is done as part of step 3 — we simply
    // never emit those tables. Listed here for parity with the Python
    // option naming.
    #[allow(unreachable_code)]
    let _ = &_new_ttf_bytes;

    // Step 5: TTF → WOFF2.
    //
    // TODO(api): confirm the exact entry point of the `woff2` crate. Likely
    // candidates (depending on the version pinned in Cargo.toml):
    //   - `woff2::convert_ttf_to_woff2(&ttf_bytes) -> Result<Vec<u8>, _>`
    //   - `woff2::encode::convert_ttf_to_woff2(...)`
    //   - `woff2::compress(...)`
    #[allow(unreachable_code)]
    let woff2_bytes: Vec<u8> = {
        // TODO(api): woff2::convert_ttf_to_woff2(&new_ttf_bytes)?
        Vec::new()
    };

    // Mirror Python's `out_path.parent.mkdir(parents=True, exist_ok=True)`.
    #[allow(unreachable_code)]
    if let Some(parent) = _out_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {}", parent.display()))?;
    }

    #[allow(unreachable_code)]
    std::fs::write(_out_path, &woff2_bytes)
        .with_context(|| format!("writing WOFF2 to {}", _out_path.display()))?;

    #[allow(unreachable_code)]
    Ok(())
}

/// Convert a TTF to a single full-cmap WOFF2 (no subset).
///
/// Mirrors `build_full_woff2` in `source/src/webfont/build.py`. Used for the
/// single-Regular benchmark baseline.
pub fn build_full_woff2(src_ttf: &Path, _out_path: &Path) -> Result<()> {
    let ttf_bytes = std::fs::read(src_ttf)
        .with_context(|| format!("reading source TTF {}", src_ttf.display()))?;

    // TTF → WOFF2 compression. No subsetting, no table edits.
    //
    // TODO(api): confirm the exact entry point of the `woff2` crate; see the
    // matching note in `build_woff2_subset`. The Python reference does this
    // implicitly by setting `font.flavor = "woff2"` and calling `font.save`.
    let _woff2_bytes: Vec<u8> = {
        // TODO(api): woff2::convert_ttf_to_woff2(&ttf_bytes)?
        let _ = &ttf_bytes;
        return Err(anyhow::anyhow!(
            "build_full_woff2: woff2 encoder not yet wired up (TODO(api))"
        ));
    };

    #[allow(unreachable_code)]
    if let Some(parent) = _out_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {}", parent.display()))?;
    }

    #[allow(unreachable_code)]
    std::fs::write(_out_path, &_woff2_bytes)
        .with_context(|| format!("writing WOFF2 to {}", _out_path.display()))?;

    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Compile-only smoke test: the public function signatures exist and
    /// match the documented shape. We don't actually execute them because
    /// the subsetter / WOFF2 encoder are still `TODO(impl)` / `TODO(api)`.
    #[test]
    fn signatures_compile() {
        let _subset_fn: fn(&Path, &Path, &[u32]) -> Result<()> = build_woff2_subset;
        let _full_fn: fn(&Path, &Path) -> Result<()> = build_full_woff2;

        // Touch a `PathBuf` so the import is used in builds where the
        // signature coercions above get optimised away.
        let _p = PathBuf::from("/dev/null");
    }
}
