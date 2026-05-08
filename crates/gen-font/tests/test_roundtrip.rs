//! Integration test: prove the basic `skrifa` -> `write-fonts` read/write
//! roundtrip works on a real variable font.
//!
//! The test reads the upstream Noto Sans JP variable font directly from
//! `../source/vendor/`. Per the workspace's "no vendored fonts" policy
//! (see CLAUDE.md), we do *not* copy that file under `rust/`. As a
//! consequence the test must tolerate the file being absent — for example
//! when the workspace is checked out without `../source/` next to it. In
//! that case the test self-skips via `eprintln!` + early return rather
//! than failing, so CI environments that don't ship the vendor tree stay
//! green.
//!
//! ## What "roundtrip" means here
//!
//! `write-fonts` 0.42 does not (yet) expose a single "load every table from
//! a skrifa source" entry point. The closest available primitive is
//! `FontBuilder::copy_missing_tables`, which copies raw table bytes from a
//! `read_fonts::FontRef` into the builder. That gives us a *binary-level*
//! roundtrip: parse, copy each table opaquely, re-serialise.
//!
//! This is **not** a true semantic roundtrip — `write-fonts` never decodes
//! the table payloads, so any structural-level lossless guarantee would
//! need per-table `FontWrite` round-trips that the 0.42 API surface
//! doesn't make uniformly available. We document that gap inline below and
//! treat the byte-copy path as the strongest claim we can make today.
//!
//! ## What we assert
//!
//! 1. Read-side facts via `skrifa` / `read_fonts::TableProvider`:
//!    - cmap entry count is non-zero and plausible for a JP font
//!    - glyph count matches `maxp.num_glyphs`
//!    - `hhea.ascender` and `OS/2.s_typo_ascender` are positive and sane
//! 2. Write-side: rebuilding the font via `FontBuilder::copy_missing_tables`
//!    + `build()` produces output whose length is within a small tolerance
//!    of the input.
//!
//! The output is not byte-identical because `head.checkSumAdjustment` is
//! recomputed, table order follows write-fonts' recommended-order policy
//! (which may differ from the source), and padding between tables is
//! normalised to 4-byte boundaries. A length within ~5% of the input is the
//! practical bound; large deviations indicate dropped tables, which would be
//! a regression.

use std::path::Path;

use skrifa::{charmap::Charmap, raw::TableProvider, FontRef};
use write_fonts::FontBuilder;

/// Path to the upstream variable font. Lives under `../source/vendor/`
/// because vendor fonts are not duplicated into `rust/` (see CLAUDE.md).
const NOTO_VARIABLE_PATH: &str =
    "../../../source/vendor/fonts/Noto_Sans_JP/NotoSansJP-VariableFont_wght.ttf";

#[test]
fn skrifa_write_fonts_roundtrip_on_noto_variable() {
    // The vendor tree is not part of this workspace's checkout. If it's
    // missing, self-skip rather than fail — the test is still useful in
    // any environment where the file *is* present, and CI that lacks the
    // file shouldn't go red on a "missing fixture" failure.
    let path = Path::new(NOTO_VARIABLE_PATH);
    if !path.is_file() {
        eprintln!(
            "skipping skrifa_write_fonts_roundtrip_on_noto_variable: \
             {NOTO_VARIABLE_PATH} not found (vendor tree absent)"
        );
        return;
    }

    let bytes = std::fs::read(path).expect("read Noto variable font");
    let font = FontRef::new(&bytes).expect("parse Noto variable font with skrifa");

    // ---- Step 2: enumerate basic facts via skrifa / read_fonts. ----

    // cmap entries — Charmap::mappings yields (codepoint, GlyphId) pairs
    // for the selected best subtable.
    let charmap = Charmap::new(&font);
    let cmap_entries = charmap.mappings().count();
    assert!(
        cmap_entries > 1000,
        "cmap entry count suspiciously low for a JP font: {cmap_entries}"
    );

    // glyph count from maxp
    let maxp = font.maxp().expect("maxp table present");
    let num_glyphs = maxp.num_glyphs();
    assert!(
        num_glyphs > 1000,
        "glyph count suspiciously low for a JP font: {num_glyphs}"
    );

    // hhea ascender
    let hhea = font.hhea().expect("hhea table present");
    let hhea_ascender: i16 = hhea.ascender().to_i16();
    assert!(
        hhea_ascender > 0,
        "expected positive hhea ascender, got {hhea_ascender}"
    );

    // OS/2 typo ascender
    let os2 = font.os2().expect("OS/2 table present");
    let os2_typo_ascender = os2.s_typo_ascender();
    assert!(
        os2_typo_ascender > 0,
        "expected positive OS/2 sTypoAscender, got {os2_typo_ascender}"
    );

    eprintln!(
        "Noto variable: cmap_entries={cmap_entries}, num_glyphs={num_glyphs}, \
         hhea.ascender={hhea_ascender}, OS/2.sTypoAscender={os2_typo_ascender}"
    );

    // ---- Step 3+4: roundtrip via FontBuilder::copy_missing_tables. ----
    //
    // TODO(api): if/when write-fonts grows a "rebuild semantically from
    // skrifa source" entry point, switch this to that — copy_missing_tables
    // is byte-opaque and so doesn't actually exercise the per-table
    // FontWrite/Validate path. See docs/WRITE_FONTS_NOTES.md for the
    // running list of API gaps we've worked around.
    let mut builder = FontBuilder::new();
    builder.copy_missing_tables(font.clone());
    let rebuilt = builder.build();

    // The rebuilt font won't be byte-identical (see module docs) but its
    // length must be within a small tolerance of the input — anything
    // larger means tables were dropped or the table directory layout
    // changed materially.
    let input_len = bytes.len();
    let output_len = rebuilt.len();
    let diff = input_len.abs_diff(output_len);
    let tolerance = input_len / 20; // 5%
    assert!(
        diff <= tolerance,
        "rebuilt font length deviates too far from input: \
         input={input_len}, output={output_len}, diff={diff}, tolerance={tolerance}"
    );

    // Sanity-check the rebuilt bytes parse back as a font and expose the
    // same glyph count — i.e. the table directory survived rebuild.
    let rebuilt_font = FontRef::new(&rebuilt).expect("re-parse rebuilt font");
    let rebuilt_num_glyphs = rebuilt_font
        .maxp()
        .expect("maxp present in rebuilt font")
        .num_glyphs();
    assert_eq!(
        rebuilt_num_glyphs, num_glyphs,
        "rebuild lost or rewrote maxp.num_glyphs"
    );
}
