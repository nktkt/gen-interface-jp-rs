//! Integration test: exercise `gen_font::strip_extreme::strip_extreme_glyphs`
//! against the real Noto Sans JP variable font.
//!
//! The pass is meant to neutralise vertical-only kana iteration marks (〱 /
//! 〲 — see `crates/gen-font/src/strip_extreme.rs` and the "Vertical metrics
//! policy" section of `rust/CLAUDE.md`). U+3031 〱 is the canonical worst
//! offender on Noto JP — its bbox extends well past the em-square. The test
//! builds the source-side facts (gid, bbox is actually extreme) before
//! invoking the pass, runs the pass into a fresh `FontBuilder` seeded via
//! `copy_missing_tables`, then re-parses the output and verifies:
//!
//!   - the glyph slot for U+3031's gid still exists but is now empty,
//!   - the codepoint U+3031 either no longer maps or maps to .notdef (gid 0).
//!
//! Per workspace policy (CLAUDE.md, "What is NOT ported") the vendor font is
//! NOT duplicated under `rust/`. We point at `../source/vendor/...` and
//! self-skip when the file is absent so checkouts without a sibling
//! `../source/` tree stay green.

use std::path::Path;

use skrifa::{raw::TableProvider, FontRef, MetadataProvider};
use write_fonts::FontBuilder;

/// Path to the upstream Noto Sans JP variable font. Lives under
/// `../source/vendor/` because vendor fonts are not duplicated into `rust/`
/// (see CLAUDE.md). Mirrors the constant in `test_roundtrip.rs`.
const NOTO_VARIABLE_PATH: &str =
    "../../../source/vendor/fonts/Noto_Sans_JP/NotoSansJP-VariableFont_wght.ttf";

/// Codepoint we expect `strip_extreme_glyphs` to neutralise on Noto JP.
/// 〱 is the vertical kana iteration mark — its glyph reaches several
/// hundred units past the em-square, dominating `head.yMax`/`yMin`. See
/// the module-level comment of `strip_extreme.rs` for full rationale.
const U_3031: u32 = 0x3031;

#[test]
fn strip_extreme_neutralises_kana_iteration_marks_on_noto() {
    // ---- Step 0: locate fixture; self-skip if vendor tree absent. -------
    let path = Path::new(NOTO_VARIABLE_PATH);
    if !path.is_file() {
        eprintln!(
            "skipping strip_extreme_neutralises_kana_iteration_marks_on_noto: \
             {NOTO_VARIABLE_PATH} not found (vendor tree absent)"
        );
        return;
    }

    let bytes = std::fs::read(path).expect("read Noto variable font");
    let font = FontRef::new(&bytes).expect("parse Noto variable font with skrifa");

    // ---- Step 1: identify gid for U+3031 〱 via charmap. ----------------
    //
    // The pass-under-test discovers targets purely by bbox; this lookup
    // is the test's independent way of saying "the iteration mark glyph
    // is what got hit", matching how the Python reference describes the
    // behaviour rather than how it implements it.
    let charmap = font.charmap();
    let Some(target_gid) = charmap.map(U_3031) else {
        // If U+3031 isn't even in the source, this fixture is no
        // longer the canonical Noto JP build the test was written
        // against. Skip rather than fail — re-running on a stripped
        // subset of Noto is a reasonable thing to want.
        eprintln!(
            "skipping strip_extreme_neutralises_kana_iteration_marks_on_noto: \
             U+3031 not present in source charmap"
        );
        return;
    };
    let target_gid_u32 = target_gid.to_u32();
    eprintln!("U+3031 〱 -> gid {target_gid_u32}");

    // ---- Step 2: confirm its bbox is actually extreme. ------------------
    //
    // If for some reason the input doesn't have an extreme-bbox iteration
    // mark, the rest of the test isn't meaningful (the pass would correctly
    // do nothing). Skip in that case — same reasoning as the charmap-miss
    // branch above.
    let glyf = font.glyf().expect("glyf table present");
    let loca = font.loca(None).expect("loca table present");
    let src_glyph = loca
        .get_glyf(target_gid, &glyf)
        .expect("read glyf for U+3031")
        .expect("U+3031 has a non-empty glyf entry");
    let src_y_max = i32::from(src_glyph.y_max());
    let src_y_min = i32::from(src_glyph.y_min());
    eprintln!("source bbox for gid {target_gid_u32}: yMax={src_y_max}, yMin={src_y_min}");
    if !(src_y_max > 1200 || src_y_min < -400) {
        eprintln!(
            "skipping strip_extreme_neutralises_kana_iteration_marks_on_noto: \
             source bbox for U+3031 is not in the extreme band \
             (yMax={src_y_max}, yMin={src_y_min}); test isn't useful here"
        );
        return;
    }

    // ---- Step 3: seed a builder via copy_missing_tables, then run the
    //              pass-under-test. Same builder shape used by the rest
    //              of the Stage 2 pipeline (see `proportional.rs`).
    let mut builder = FontBuilder::new();
    builder.copy_missing_tables(font.clone());

    let count = gen_font::strip_extreme::strip_extreme_glyphs(&font, &mut builder)
        .expect("strip_extreme_glyphs ran without error");
    assert!(
        count > 0,
        "expected strip_extreme_glyphs to neutralise at least one glyph, got {count}"
    );
    eprintln!("strip_extreme_glyphs neutralised {count} glyph(s)");

    // ---- Step 4: build new bytes, reparse, and verify post-conditions. --
    let rebuilt = builder.build();
    let new_font = FontRef::new(&rebuilt).expect("reparse rebuilt font");

    // 4a. cmap: the codepoint should drop entirely (or fall to gid 0).
    let new_charmap = new_font.charmap();
    match new_charmap.map(U_3031) {
        None => {
            // Expected path for Noto JP: `Cmap::from_mappings` regenerates
            // a canonical pair of subtables and the dropped entry is gone.
            eprintln!("post-strip: U+3031 no longer mapped (good)");
        }
        Some(gid) if gid.to_u32() == 0 => {
            eprintln!("post-strip: U+3031 maps to .notdef (good)");
        }
        Some(gid) => panic!(
            "U+3031 still maps to a non-notdef glyph after strip: gid {}",
            gid.to_u32()
        ),
    }

    // 4b. glyf: the original target gid is still in range (we preserved
    //     the slot), but its outline is now empty — bbox collapsed to the
    //     write-fonts canonical empty-glyph (yMax/yMin/xMax/xMin all 0)
    //     OR the slot returns `None` (no glyf bytes written for an Empty).
    let new_glyf = new_font.glyf().expect("glyf in rebuilt");
    let new_loca = new_font.loca(None).expect("loca in rebuilt");
    let new_glyph_opt = new_loca
        .get_glyf(target_gid, &new_glyf)
        .expect("read rebuilt glyf for original target gid");
    match new_glyph_opt {
        None => {
            // `Glyph::Empty` writes no bytes into glyf and a zero-length
            // loca run — `get_glyf` reports that as `Ok(None)`.
            eprintln!("post-strip: gid {target_gid_u32} is an empty slot (good)");
        }
        Some(g) => {
            let y_max = i32::from(g.y_max());
            let y_min = i32::from(g.y_min());
            let x_max = i32::from(g.x_max());
            let x_min = i32::from(g.x_min());
            assert_eq!(
                (x_min, y_min, x_max, y_max),
                (0, 0, 0, 0),
                "post-strip: gid {target_gid_u32} kept a non-empty bbox \
                 (xMin={x_min}, yMin={y_min}, xMax={x_max}, yMax={y_max})"
            );
            eprintln!("post-strip: gid {target_gid_u32} bbox collapsed to (0,0,0,0) (good)");
        }
    }
}
