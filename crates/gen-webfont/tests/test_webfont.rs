use gen_webfont::ranges::{
    codepoints_from_ranges, format_unicode_range, is_han_codepoint,
    merge_codepoints_to_ranges, JP_KANA_RANGES, LATIN_RANGES,
};

#[test]
fn merge_codepoints_to_ranges_collapses_runs() {
    assert_eq!(
        merge_codepoints_to_ranges([0x41, 0x42, 0x43]),
        vec![(0x41, 0x43)]
    );
    assert_eq!(
        merge_codepoints_to_ranges([0x41, 0x43]),
        vec![(0x41, 0x41), (0x43, 0x43)]
    );
    assert_eq!(merge_codepoints_to_ranges(std::iter::empty::<u32>()), vec![]);
}

#[test]
fn merge_codepoints_to_ranges_dedupes_and_sorts() {
    assert_eq!(
        merge_codepoints_to_ranges([0x43, 0x41, 0x42, 0x42]),
        vec![(0x41, 0x43)]
    );
}

#[test]
fn format_unicode_range_singletons_ranges_and_5_digit() {
    assert_eq!(format_unicode_range([0x41]), "U+0041");
    assert_eq!(format_unicode_range([0x41, 0x42, 0x43]), "U+0041-0043");
    assert_eq!(format_unicode_range([0x41, 0x43]), "U+0041, U+0043");
    assert_eq!(format_unicode_range([0x1F130]), "U+1F130");
}

#[test]
fn is_han_codepoint_blocks() {
    assert!(is_han_codepoint(0x4E00));
    assert!(is_han_codepoint(0x3400));
    assert!(is_han_codepoint(0x2FA1F));
    assert!(!is_han_codepoint(0x3042));
    assert!(!is_han_codepoint(0x41));
}

#[test]
fn codepoints_from_ranges_expands() {
    let s = codepoints_from_ranges(&[(0x41, 0x42)]);
    assert_eq!(s.len(), 2);
    assert!(s.contains(&0x41));
    assert!(s.contains(&0x42));
}

#[test]
fn latin_and_kana_range_constants_are_nonempty() {
    assert!(!LATIN_RANGES.is_empty());
    assert!(!JP_KANA_RANGES.is_empty());
}

use gen_webfont::jis::jis_row_codepoints;

#[test]
fn jis_row_16_includes_first_kanji() {
    let cps = jis_row_codepoints(16);
    assert!(!cps.is_empty());
    assert!(cps.contains(&0x4E9C)); // 亜
}

use gen_webfont::plan::*;

#[test]
fn build_subset_plan_no_overlap_and_full_coverage() {
    // Use a small mock cmap mixing Latin / kana / Han
    let cps = vec![0x41, 0x42, 0x3042, 0x4E00];
    let plan = build_subset_plan(cps.iter().copied(), 4);
    let mut all_cps = std::collections::BTreeSet::new();
    for s in &plan {
        for cp in &s.codepoints {
            assert!(all_cps.insert(*cp), "duplicate cp {cp:#X} across subsets");
        }
    }
    assert_eq!(all_cps.into_iter().collect::<Vec<_>>(), cps);
}

use gen_webfont::google_japanese::parse_slicing_strategy;

#[test]
fn parse_slicing_strategy_handles_brace_in_comment() {
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("gen_webfont_slicing_test_{}.txt", std::process::id()));
    let body = "subsets {\n  codepoints: 0x7D # } RIGHT CURLY BRACKET\n}\n";
    std::fs::write(&tmp, body).unwrap();
    let parsed = parse_slicing_strategy(&tmp).unwrap();
    assert_eq!(parsed.len(), 1);
    assert!(parsed[0].contains(&0x7D));
    let _ = std::fs::remove_file(&tmp);
}

use gen_webfont::css::{font_face_css, font_face_css_minified, weight_css_filename};

#[test]
fn weight_css_filename_normal_vs_display() {
    assert_eq!(weight_css_filename("normal", 400), "400.css");
    assert_eq!(weight_css_filename("display", 800), "display-800.css");
}

#[test]
fn font_face_css_has_unicode_range_when_provided() {
    let s = font_face_css("Gen Interface JP", 400, "./r.woff2", Some("U+0041"));
    assert!(s.contains("font-family: \"Gen Interface JP\""));
    assert!(s.contains("font-weight: 400"));
    assert!(s.contains("unicode-range: U+0041"));
    let s2 = font_face_css("Gen Interface JP", 400, "./r.woff2", None);
    assert!(!s2.contains("unicode-range"));
}

#[test]
fn font_face_css_minified_is_single_line() {
    let s = font_face_css_minified("Gen Interface JP", 700, "./b.woff2", "U+0041");
    assert!(!s.contains('\n'));
}
