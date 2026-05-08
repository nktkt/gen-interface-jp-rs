//! Integration test: run [`gen_font::palt::read_palt`] against the real
//! Noto Sans JP variable font.
//!
//! Per the workspace's "no vendored fonts" policy (see CLAUDE.md), the
//! upstream font is read directly from `../source/vendor/`. If the file
//! is absent (e.g. the workspace is checked out without `../source/`),
//! the tests self-skip via `eprintln!` + early return rather than
//! failing.
//!
//! ## What we assert
//!
//! - [`read_palt_on_noto_variable_is_non_empty`] — the primary signal
//!   that the implementation isn't the previous `Ok(empty)` stub. Noto
//!   Sans JP definitely has palt entries for kana and CJK punctuation,
//!   so an empty map means the implementation regressed. Also prints a
//!   summary of entry count and xp / xa ranges for human inspection.
//! - [`read_palt_known_entries_hiragana_a_and_ideographic_comma`] — spot
//!   checks for U+3042 あ and U+3001 、 by resolving their codepoints
//!   through the font's cmap to glyph ids and looking those up in the
//!   `read_palt` output. `read_palt` keys by gid (not by glyph name) so
//!   the test is robust against fonts whose `post` table doesn't expose
//!   `uniXXXX` strings — Noto Sans JP ships a format-3 `post` whose
//!   names skrifa surfaces as synthetic `gidNNN` placeholders.

use std::path::Path;

use skrifa::{FontRef, MetadataProvider};

use gen_font::palt::read_palt;

/// Path to the upstream variable font. Lives under `../source/vendor/`
/// because vendor fonts are not duplicated into `rust/` (see CLAUDE.md).
const NOTO_VARIABLE_PATH: &str =
    "../../../source/vendor/fonts/Noto_Sans_JP/NotoSansJP-VariableFont_wght.ttf";

/// Load the upstream Noto variable font, or return `None` if the vendor
/// tree is absent (CI environments that don't ship it should self-skip).
fn load_noto_variable_bytes() -> Option<Vec<u8>> {
    let path = Path::new(NOTO_VARIABLE_PATH);
    if !path.is_file() {
        eprintln!(
            "skipping palt integration test: {NOTO_VARIABLE_PATH} not found \
             (vendor tree absent)"
        );
        return None;
    }
    Some(std::fs::read(path).expect("read Noto variable font"))
}

#[test]
fn read_palt_on_noto_variable_is_non_empty() {
    let Some(bytes) = load_noto_variable_bytes() else {
        return;
    };
    let font = FontRef::new(&bytes).expect("parse Noto variable font with skrifa");

    let palt = read_palt(&font).expect("read_palt returned an error");

    // Non-empty: the previous stub returned `Ok(empty)` and we want
    // that regression to surface here. Noto Sans JP has hundreds of
    // palt entries for kana / punctuation.
    assert!(
        !palt.is_empty(),
        "read_palt returned an empty map — Noto Sans JP has palt entries; \
         the implementation may have regressed to the stub"
    );

    // Sanity-check the magnitude: a JP font's palt feature should cover
    // many dozens of glyphs at minimum. The exact number is font-specific
    // (Noto Sans JP currently lands around 300+).
    assert!(
        palt.len() >= 50,
        "palt map only has {} entries — suspiciously low for Noto Sans JP",
        palt.len()
    );

    // ---- Summary printout for human inspection. ----
    let count = palt.len();
    let xp_min = palt.values().map(|(xp, _)| *xp).min().unwrap_or(0);
    let xp_max = palt.values().map(|(xp, _)| *xp).max().unwrap_or(0);
    let xa_min = palt.values().map(|(_, xa)| *xa).min().unwrap_or(0);
    let xa_max = palt.values().map(|(_, xa)| *xa).max().unwrap_or(0);
    eprintln!(
        "Noto Sans JP palt: entries={count}, \
         xp range=[{xp_min}, {xp_max}], xa range=[{xa_min}, {xa_max}]"
    );

    // The map should contain at least one entry whose xp or xa is
    // strictly negative — palt for CJK punctuation routinely shrinks
    // the slot leftward, so an all-zero / all-positive map means
    // value-record decoding is wrong.
    let any_negative = palt.values().any(|(xp, xa)| *xp < 0 || *xa < 0);
    assert!(
        any_negative,
        "expected at least one palt entry with negative xp or xa; \
         all-positive values suggest sign-handling regression"
    );
}

#[test]
fn read_palt_known_entries_hiragana_a_and_ideographic_comma() {
    let Some(bytes) = load_noto_variable_bytes() else {
        return;
    };
    let font = FontRef::new(&bytes).expect("parse Noto variable font with skrifa");

    let palt = read_palt(&font).expect("read_palt returned an error");

    // The map is keyed by glyph id; resolve U+3042 / U+3001 through the
    // font's cmap to get the gids we need to look up. This avoids
    // depending on the `post` table's name strings (Noto Sans JP ships
    // format-3 `post` and skrifa returns synthetic `gidNNN`).
    let charmap = font.charmap();

    // U+3042 あ — hiragana A. Exact (xp, xa) values depend on the font;
    // just assert presence and that at least one component is non-zero.
    let gid_a = charmap
        .map(0x3042u32)
        .expect("U+3042 あ should be in cmap")
        .to_u32();
    let (xp_a, xa_a) = *palt
        .get(&gid_a)
        .expect("expected gid for U+3042 あ in palt map");
    assert!(
        xp_a != 0 || xa_a != 0,
        "palt for U+3042 あ (gid={gid_a}) is (0, 0) — expected at least \
         one non-zero component, got xp={xp_a}, xa={xa_a}"
    );
    eprintln!("U+3042 あ (gid={gid_a}) -> (xp={xp_a}, xa={xa_a})");

    // U+3001 、 — ideographic comma. palt for opening / closing CJK
    // punctuation typically shrinks the slot leftward, i.e. negative
    // xp and negative xa.
    let gid_c = charmap
        .map(0x3001u32)
        .expect("U+3001 、 should be in cmap")
        .to_u32();
    let (xp_c, xa_c) = *palt
        .get(&gid_c)
        .expect("expected gid for U+3001 、 in palt map");
    eprintln!("U+3001 、 (gid={gid_c}) -> (xp={xp_c}, xa={xa_c})");
    assert!(
        xp_c < 0,
        "expected negative xp for U+3001 、 (palt shrinks the slot \
         leftward), got xp={xp_c}"
    );
    assert!(
        xa_c < 0,
        "expected negative xa for U+3001 、 (palt shrinks the slot \
         leftward), got xa={xa_c}"
    );
}
