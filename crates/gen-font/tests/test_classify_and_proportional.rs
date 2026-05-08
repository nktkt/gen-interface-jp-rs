//! Integration tests for `gen-font` covering classify, palt, `glyph_spacing`,
//! and weight-table logic. Loosely ported from the Python tests in
//! `tests/test_font_build.py` and `tests/test_proportional.py` — only the
//! parts that don't need real font fixtures.

use gen_font::classify::*;

#[test]
fn glyph_codepoint_parses_uni_form() {
    assert_eq!(glyph_codepoint("uni3042"), Some(0x3042));
    assert_eq!(glyph_codepoint("uni3042.alt"), Some(0x3042));
    assert_eq!(glyph_codepoint("A"), None);
    assert_eq!(glyph_codepoint("uni"), None);
    assert_eq!(glyph_codepoint("uniZZZZ"), None);
}

#[test]
fn is_kana_or_punct_covers_blocks() {
    assert!(is_kana_or_punct("uni3042")); // hiragana
    assert!(is_kana_or_punct("uni30A2")); // katakana
    assert!(is_kana_or_punct("uni3000")); // CJK punctuation
    assert!(is_kana_or_punct("uniFF21")); // fullwidth A
    assert!(!is_kana_or_punct("A"));
    assert!(!is_kana_or_punct("uni4E00")); // Han
}

#[test]
fn is_kana_letter_excludes_middle_dot() {
    // U+30FB ・ is katakana-block punctuation, NOT a letter.
    assert!(!is_kana_letter("uni30FB"));
    assert!(is_kana_letter("uni30FA")); // ヺ (last katakana letter)
    assert!(is_kana_letter("uni30FC")); // ー prolonged sound
}

#[test]
fn is_cjk_codepoint_blocks() {
    assert!(is_cjk_codepoint(0x4E00));
    assert!(is_cjk_codepoint(0x6F22));
    assert!(!is_cjk_codepoint(0x3042));
    assert!(is_cjk_codepoint(0x20000)); // Extension B
    assert!(is_cjk_codepoint(0x2FA1F)); // last in Extension F+
    assert!(!is_cjk_codepoint(0x2FA20));
}

use gen_font::weights::*;

#[test]
fn weights_table_size_and_lookup() {
    assert_eq!(WEIGHTS.len(), 8);
    assert_eq!(WEIGHTS[0].weight_num, 100);
    assert_eq!(WEIGHTS[3].weight_num, 400);
    assert_eq!(WEIGHTS[3].weight_name, "Regular");
    assert_eq!(WEIGHTS[3].noto_wght_axis, 465);
    assert_eq!(WEIGHTS[7].weight_name, "ExtraBold");
}

#[test]
fn weights_lookup_by_name_or_num() {
    assert_eq!(
        WeightSpec::by_name_or_num("Regular").map(|w| w.weight_num),
        Some(400)
    );
    assert_eq!(
        WeightSpec::by_name_or_num("400").map(|w| w.weight_name),
        Some("Regular")
    );
    assert!(WeightSpec::by_name_or_num("Nonsense").is_none());
}

use gen_font::families::*;

#[test]
fn families_table() {
    let normal = FamilyConfig::lookup("normal").expect("normal family");
    assert_eq!(normal.family_name, "Gen Interface JP");
    assert_eq!(normal.tracking, 30);
    assert_eq!(normal.tracking_kana, Some(40));
    assert_eq!(normal.glyph_spacing, &[('く', (30, 0))]);
    let display = FamilyConfig::lookup("display").expect("display family");
    assert_eq!(display.family_name, "Gen Interface JP Display");
    assert_eq!(display.tracking, 0);
    assert!(FamilyConfig::lookup("nonsense").is_none());
}

#[test]
fn baseline_offset_and_scale() {
    assert_eq!(BASELINE_OFFSET, 25);
    assert!((SCALE - 0.925).abs() < 1e-6);
}

use gen_font::baker::parse_codepoint_spec;

#[test]
fn parse_codepoint_spec_singletons_and_ranges() {
    assert_eq!(
        parse_codepoint_spec(&["U+25CE".into()]).unwrap(),
        vec![0x25CE]
    );
    let r = parse_codepoint_spec(&["U+2460-U+2462".into()]).unwrap();
    assert_eq!(r, vec![0x2460, 0x2461, 0x2462]);
    let mixed = parse_codepoint_spec(&["U+0041".into(), "U+2460-U+2461".into()]).unwrap();
    assert_eq!(mixed, vec![0x41, 0x2460, 0x2461]);
    assert!(parse_codepoint_spec(&["bogus".into()]).is_err());
}
