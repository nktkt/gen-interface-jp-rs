//! Integration test: run `gen_font::tracking::apply_tracking` against the
//! real Noto Sans JP variable font and verify the resulting font has wider
//! advance widths.
//!
//! Per CLAUDE.md ("no vendored fonts") the upstream variable font lives
//! under `../source/vendor/`, NOT inside this workspace. If the file is
//! absent (e.g. CI checkouts that don't ship the vendor tree), the test
//! self-skips via `eprintln!` + early return instead of failing.
//!
//! ## What this asserts
//!
//! 1. Pick three representative glyph ids by codepoint:
//!    - GID 0 (`.notdef`)
//!    - the GID for `あ` (U+3042) — a kana letter
//!    - the GID for `A` (U+0041) — a Latin letter
//! 2. Read each glyph's advance width from the source `hmtx`.
//! 3. Build a fresh font: `FontBuilder::copy_missing_tables` to seed every
//!    table from source, then `apply_tracking(font, builder, 50, Some(60))`
//!    to overwrite `hmtx` / `hhea` with tracked metrics.
//! 4. Re-parse the rebuilt bytes and read the same glyphs' advance widths.
//! 5. Assert each glyph's advance grew by the per-class tracking value
//!    (50 design units for Latin / .notdef, 60 for kana).
//!
//! ## Kana classification via cmap
//!
//! `apply_tracking` precomputes the kana / CJK-punctuation glyph set by
//! walking the font's cmap (see `classify::get_kana_or_punct_glyphs`),
//! which is robust against fonts whose `post` table doesn't expose
//! Adobe-style `uniXXXX` glyph names. Noto Sans JP is format-3 `post`
//! (no name list), so a glyph-name-driven classifier wouldn't fire on
//! `あ`; the cmap-driven path catches it correctly.

use std::path::Path;

use skrifa::{charmap::Charmap, raw::TableProvider, FontRef, GlyphId};
use write_fonts::FontBuilder;

/// Path to the upstream variable font. Lives under `../source/vendor/`
/// because vendor fonts are not duplicated into `rust/` (see CLAUDE.md).
const NOTO_VARIABLE_PATH: &str =
    "../../../source/vendor/fonts/Noto_Sans_JP/NotoSansJP-VariableFont_wght.ttf";

const TRACKING_LATIN: i32 = 50;
const TRACKING_KANA: i32 = 60;

/// Look up `gid`'s advance width in `font`'s `hmtx`, accounting for the
/// trailing implicit-advance tail (gids beyond `number_of_h_metrics` share
/// the last long metric's advance).
fn advance_for_gid(font: &FontRef<'_>, gid: u32) -> u16 {
    let hmtx = font.hmtx().expect("read hmtx");
    let hhea = font.hhea().expect("read hhea");
    let num_long = usize::from(hhea.number_of_h_metrics());
    let h_metrics = hmtx.h_metrics();
    if (gid as usize) < num_long {
        h_metrics[gid as usize].advance()
    } else {
        // Trailing glyphs share the last long metric's advance.
        h_metrics.last().map_or(0, |m| m.advance())
    }
}

/// Returns `(font_bytes, tracked_font_bytes)` after applying tracking, or
/// `None` if the vendor font is missing (caller self-skips).
///
/// Both byte buffers are returned by-value so the caller can re-parse them
/// independently — a `FontRef` borrows its source bytes, and we need
/// independent lifetimes for the original and the rebuilt font.
fn apply_tracking_to_noto() -> Option<(Vec<u8>, Vec<u8>)> {
    let path = Path::new(NOTO_VARIABLE_PATH);
    if !path.is_file() {
        eprintln!(
            "skipping tracking integration test: \
             {NOTO_VARIABLE_PATH} not found (vendor tree absent)"
        );
        return None;
    }

    let bytes = std::fs::read(path).expect("read Noto variable font");
    let font = FontRef::new(&bytes).expect("parse Noto variable font with skrifa");

    let mut builder = FontBuilder::new();
    builder.copy_missing_tables(font.clone());
    gen_font::tracking::apply_tracking(&font, &mut builder, TRACKING_LATIN, Some(TRACKING_KANA))
        .expect("apply_tracking succeeds on Noto variable");
    let new_bytes = builder.build();

    Some((bytes, new_bytes))
}

#[test]
fn test_tracking_widens_latin_and_notdef_advances() {
    let Some((bytes, new_bytes)) = apply_tracking_to_noto() else {
        return;
    };

    let font = FontRef::new(&bytes).expect("parse source");
    let new_font = FontRef::new(&new_bytes).expect("re-parse rebuilt font");

    let charmap = Charmap::new(&font);
    let gid_notdef: GlyphId = GlyphId::new(0);
    let gid_a_latin: GlyphId = charmap
        .map(0x0041u32)
        .expect("U+0041 (A) must be in the cmap");

    let original_advance_notdef = advance_for_gid(&font, gid_notdef.to_u32());
    let original_advance_latin = advance_for_gid(&font, gid_a_latin.to_u32());

    let new_advance_notdef = advance_for_gid(&new_font, gid_notdef.to_u32());
    let new_advance_latin = advance_for_gid(&new_font, gid_a_latin.to_u32());

    eprintln!(
        "notdef(gid {}): {} -> {}; A(gid {}): {} -> {}",
        gid_notdef.to_u32(),
        original_advance_notdef,
        new_advance_notdef,
        gid_a_latin.to_u32(),
        original_advance_latin,
        new_advance_latin,
    );

    assert!(
        original_advance_notdef > 0,
        ".notdef advance is zero, test fixture invalid"
    );
    assert!(
        original_advance_latin > 0,
        "A advance is zero, test fixture invalid"
    );

    // .notdef has glyph name `.notdef`, which `is_kana_or_punct` returns
    // false for, so it gets the Latin tracking value.
    let expected_notdef = u32::from(original_advance_notdef) + TRACKING_LATIN as u32;
    assert_eq!(
        u32::from(new_advance_notdef),
        expected_notdef,
        ".notdef advance did not grow by Latin tracking value: \
         original={original_advance_notdef}, new={new_advance_notdef}, \
         expected={expected_notdef}"
    );

    // `A` (U+0041) is Latin; it should receive the Latin tracking value.
    // Noto Sans JP names this glyph `A`, which is not `uniXXXX` form, so
    // the classifier returns false and it gets the default branch.
    let expected_latin = u32::from(original_advance_latin) + TRACKING_LATIN as u32;
    assert_eq!(
        u32::from(new_advance_latin),
        expected_latin,
        "A advance did not grow by Latin tracking value: \
         original={original_advance_latin}, new={new_advance_latin}, \
         expected={expected_latin}"
    );
}

/// Verify that the kana tracking value is applied to `あ` (U+3042).
///
/// `apply_tracking` builds its kana set via cmap lookup (see
/// `classify::get_kana_or_punct_glyphs`), so this works on Noto Sans JP
/// even though its `post` table is format 3 and exposes no glyph names.
#[test]
fn test_tracking_widens_kana_advances_by_kana_value() {
    let Some((bytes, new_bytes)) = apply_tracking_to_noto() else {
        return;
    };

    let font = FontRef::new(&bytes).expect("parse source");
    let new_font = FontRef::new(&new_bytes).expect("re-parse rebuilt font");

    let charmap = Charmap::new(&font);
    let gid_a_kana: GlyphId = charmap
        .map(0x3042u32)
        .expect("U+3042 (あ) must be in the JP cmap");

    let original_advance_kana = advance_for_gid(&font, gid_a_kana.to_u32());
    let new_advance_kana = advance_for_gid(&new_font, gid_a_kana.to_u32());

    eprintln!(
        "あ(gid {}): {} -> {}",
        gid_a_kana.to_u32(),
        original_advance_kana,
        new_advance_kana,
    );

    assert!(
        original_advance_kana > 0,
        "あ advance is zero, test fixture invalid"
    );

    // `あ` (U+3042) is hiragana; it should receive the kana tracking value.
    let expected_kana = u32::from(original_advance_kana) + TRACKING_KANA as u32;
    assert_eq!(
        u32::from(new_advance_kana),
        expected_kana,
        "あ advance did not grow by kana tracking value: \
         original={original_advance_kana}, new={new_advance_kana}, \
         expected={expected_kana}"
    );
}
