//! Integration tests for the glyph-name-set helpers in
//! [`gen_font::classify`]: `glyph_names`, `get_vert_alternates`, and
//! `get_cjk_glyphs`.
//!
//! These exercise the real Noto Sans JP variable font from
//! `../source/vendor/` (per the no-vendored-fonts policy in CLAUDE.md). If
//! the vendor tree is absent the tests self-skip via `eprintln!` + early
//! return, mirroring `test_palt_integration.rs`.

use std::path::Path;

use skrifa::FontRef;

use gen_font::classify::{get_cjk_glyphs, get_vert_alternates, glyph_names};

const NOTO_VARIABLE_PATH: &str =
    "../../../source/vendor/fonts/Noto_Sans_JP/NotoSansJP-VariableFont_wght.ttf";

fn load_noto_variable_bytes() -> Option<Vec<u8>> {
    let path = Path::new(NOTO_VARIABLE_PATH);
    if !path.is_file() {
        eprintln!(
            "skipping classify glyph-set integration test: {NOTO_VARIABLE_PATH} \
             not found (vendor tree absent)"
        );
        return None;
    }
    Some(std::fs::read(path).expect("read Noto variable font"))
}

#[test]
fn glyph_names_returns_one_per_gid() {
    let Some(bytes) = load_noto_variable_bytes() else {
        return;
    };
    let font = FontRef::new(&bytes).expect("parse Noto variable font with skrifa");

    let names = glyph_names(&font);

    // Noto Sans JP has tens of thousands of glyphs; lower bound at 5000 to
    // catch a broken / empty result without depending on the exact count
    // (which varies by Noto release).
    assert!(
        names.len() > 5000,
        "glyph_names returned only {} entries — Noto Sans JP has many more",
        names.len()
    );
    // GID 0 is `.notdef` in every well-formed font, but Noto Sans JP's
    // `post` table is format-3 (no string table), so skrifa synthesises
    // names as `gidN`. Accept either — what we really care about is that
    // GID 0's name is non-empty and stable.
    assert!(!names.is_empty());
    assert!(
        names[0] == ".notdef" || names[0] == "gid0",
        "GID 0 should be `.notdef` or synthesised `gid0`, got `{}`",
        names[0]
    );
    // Every entry should be a non-empty string (skrifa synthesises `gidNNN`
    // for unnamed slots, which is also non-empty).
    assert!(
        names.iter().all(|n| !n.is_empty()),
        "glyph_names must never produce an empty string"
    );
}

#[test]
fn get_vert_alternates_finds_vert_targets() {
    let Some(bytes) = load_noto_variable_bytes() else {
        return;
    };
    let font = FontRef::new(&bytes).expect("parse Noto variable font with skrifa");

    let alts = get_vert_alternates(&font).expect("get_vert_alternates returned an error");

    // Noto Sans JP ships a `vert` feature covering punctuation (CJK comma,
    // period, brackets, etc.). An empty result implies the GSUB walk
    // missed the vertical lookups entirely.
    assert!(
        !alts.is_empty(),
        "get_vert_alternates returned an empty set — Noto Sans JP \
         has `vert` substitutions"
    );
    eprintln!(
        "Noto Sans JP vert alternates: {} glyph names (sample: {:?})",
        alts.len(),
        alts.iter().take(5).collect::<Vec<_>>()
    );
}

#[test]
fn get_cjk_glyphs_covers_basic_han() {
    let Some(bytes) = load_noto_variable_bytes() else {
        return;
    };
    let font = FontRef::new(&bytes).expect("parse Noto variable font with skrifa");

    let cjk = get_cjk_glyphs(&font).expect("get_cjk_glyphs returned an error");

    // Noto Sans JP covers most of CJK Unified Ideographs (U+4E00..=U+9FFF,
    // ~20k codepoints). Lower bound the result at 1000 to catch a broken
    // cmap walk; the real number is much larger.
    assert!(
        cjk.len() > 1000,
        "get_cjk_glyphs returned only {} entries — Noto Sans JP has \
         tens of thousands of CJK ideographs",
        cjk.len()
    );
    eprintln!(
        "Noto Sans JP CJK glyphs: {} glyph names (sample: {:?})",
        cjk.len(),
        cjk.iter().take(5).collect::<Vec<_>>()
    );
}
