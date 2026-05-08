//! CSS-generation helpers for `@font-face` rules.
//!
//! Ported from `source/src/webfont/build.py` (the `font_face_css`,
//! `font_face_css_minified`, and `weight_css_filename` helpers around
//! lines 383-440).
//!
//! The Python helpers use module-level constants (`FAMILY_NAME`, `WEIGHT`,
//! `STYLE`, `DISPLAY`) for `font_face_css`. To stay flexible — so the Rust
//! `build_runner` can emit one rule per (family x weight) — we accept those
//! as parameters here. Call sites should pass the Python defaults
//! ("Gen Interface JP", 400, "normal", "swap") to reproduce the Python
//! output byte-for-byte.

/// Format a single `@font-face` rule (multi-line, pretty-printed).
///
/// `family` defaults to `"Gen Interface JP"`, `weight` to `400`, `style` to
/// `"normal"`, `display` to `"swap"` in the Python — but they are exposed as
/// parameters here so the build runner can emit per (family x weight).
/// When `unicode_range` is `None`, the `unicode-range:` line is omitted.
pub fn font_face_css(
    family: &str,
    weight: u16,
    src: &str,
    unicode_range: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("@font-face {\n");
    out.push_str(&format!("  font-family: \"{}\";\n", family));
    out.push_str("  font-style: normal;\n");
    out.push_str(&format!("  font-weight: {};\n", weight));
    out.push_str("  font-display: swap;\n");
    out.push_str(&format!("  src: url(\"{}\") format(\"woff2\");\n", src));
    if let Some(ur) = unicode_range {
        out.push_str(&format!("  unicode-range: {};\n", ur));
    }
    out.push_str("}");
    out
}

/// One-line minified `@font-face` rule used for `all.css` and per-weight CSS.
///
/// Mirrors the Python `font_face_css_minified` exactly: a single space remains
/// between `url("...")` and `format("woff2")`, matching standard CSS minifiers.
pub fn font_face_css_minified(
    family: &str,
    weight: u16,
    src: &str,
    unicode_range: &str,
) -> String {
    format!(
        "@font-face{{font-family:\"{family}\";font-style:normal;font-weight:{weight};font-display:swap;src:url(\"{src}\") format(\"woff2\");unicode-range:{unicode_range};}}"
    )
}

/// Compute the per-weight CSS filename in the npm package.
///
/// `"normal"` family key produces `<weight>.css` (e.g. `400.css`); any other
/// family key (e.g. `"display"`) is prefixed: `<family_key>-<weight>.css`.
pub fn weight_css_filename(family_key: &str, weight: u16) -> String {
    if family_key == "normal" {
        format!("{weight}.css")
    } else {
        format!("{family_key}-{weight}.css")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_face_css_with_unicode_range_matches_python_output() {
        let got = font_face_css(
            "Gen Interface JP",
            400,
            "./regular.woff2",
            Some("U+0041-005A"),
        );
        let expected = "@font-face {\n  font-family: \"Gen Interface JP\";\n  font-style: normal;\n  font-weight: 400;\n  font-display: swap;\n  src: url(\"./regular.woff2\") format(\"woff2\");\n  unicode-range: U+0041-005A;\n}";
        assert_eq!(got, expected);
    }

    #[test]
    fn font_face_css_without_unicode_range_omits_line() {
        let got = font_face_css("Gen Interface JP", 400, "./regular.woff2", None);
        let expected = "@font-face {\n  font-family: \"Gen Interface JP\";\n  font-style: normal;\n  font-weight: 400;\n  font-display: swap;\n  src: url(\"./regular.woff2\") format(\"woff2\");\n}";
        assert_eq!(got, expected);
        assert!(!got.contains("unicode-range"));
    }

    #[test]
    fn font_face_css_threads_family_and_weight() {
        let got = font_face_css("Gen Display JP", 800, "./d.woff2", None);
        assert!(got.contains("font-family: \"Gen Display JP\";"));
        assert!(got.contains("font-weight: 800;"));
    }

    #[test]
    fn font_face_css_minified_is_single_line() {
        let got = font_face_css_minified("Gen Interface JP", 700, "./b.woff2", "U+0041");
        // Single line: no embedded newlines.
        assert!(!got.contains('\n'));
        // Matches the Python output exactly (note the single space between
        // `url("...")` and `format("woff2")` — same as the Python).
        let expected = "@font-face{font-family:\"Gen Interface JP\";font-style:normal;font-weight:700;font-display:swap;src:url(\"./b.woff2\") format(\"woff2\");unicode-range:U+0041;}";
        assert_eq!(got, expected);
    }

    #[test]
    fn weight_css_filename_normal_family_omits_prefix() {
        assert_eq!(weight_css_filename("normal", 400), "400.css");
        assert_eq!(weight_css_filename("normal", 700), "700.css");
    }

    #[test]
    fn weight_css_filename_other_family_uses_prefix() {
        assert_eq!(weight_css_filename("display", 800), "display-800.css");
        assert_eq!(weight_css_filename("mono", 400), "mono-400.css");
    }
}
