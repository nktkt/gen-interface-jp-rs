//! Codepoint and glyph classification helpers.
//!
//! Ported from `source/src/font/build.py`. These helpers drive the
//! tracking and proportional-metrics passes: kana and CJK punctuation
//! read at a wider rhythm than Latin, ideographs must stay on the
//! full-width grid, and only kana *letters* (not the middle dot) want
//! the full palt shrink.

/// Parse the codepoint from an Adobe-style `uniXXXX` glyph name.
///
/// Returns `None` if the name doesn't follow `uni<4 hex>`. The check is
/// deliberately tolerant: any prefix matching `uni<4 hex>` parses, even
/// if extra characters follow (Noto sometimes ships names like
/// `uni3042.alt` which we still treat as U+3042). Names lacking the
/// `uni` prefix or with non-hex characters return `None` — those glyphs
/// are excluded from kana/CJK classification rather than misclassified.
pub fn glyph_codepoint(glyph_name: &str) -> Option<u32> {
    let rest = glyph_name.strip_prefix("uni")?;
    if rest.len() < 4 {
        return None;
    }
    // Slice the first 4 bytes — `uni` plus 4 ASCII hex digits is pure
    // ASCII, so byte indexing is safe here. Anything non-ASCII in the
    // first 4 bytes will fail the hex parse below.
    let hex = rest.get(..4)?;
    u32::from_str_radix(hex, 16).ok()
}

/// Return `true` for hiragana, katakana, or CJK punctuation glyphs.
///
/// Used by tracking to apply a separate (usually larger) tracking value
/// to kana and punctuation, since they read at a wider rhythm than Latin
/// when set at the same nominal size.
///
/// Block ranges:
/// - `0x3000..=0x303F` CJK Symbols and Punctuation (。、・「」…)
/// - `0x3040..=0x309F` Hiragana
/// - `0x30A0..=0x30FF` Katakana
/// - `0x31F0..=0x31FF` Katakana Phonetic Extensions
/// - `0xFF00..=0xFFEF` Halfwidth and Fullwidth Forms
pub fn is_kana_or_punct(glyph_name: &str) -> bool {
    let Some(cp) = glyph_codepoint(glyph_name) else {
        return false;
    };
    (0x3000..=0x303F).contains(&cp)
        || (0x3040..=0x309F).contains(&cp)
        || (0x30A0..=0x30FF).contains(&cp)
        || (0x31F0..=0x31FF).contains(&cp)
        || (0xFF00..=0xFFEF).contains(&cp)
}

/// Return `true` for hiragana / katakana *letters*, excluding punctuation.
///
/// Stricter than [`is_kana_or_punct`]: the kana proportional pass keeps
/// full palt shrink on letters (where palt's optical kerning is designed
/// to apply) and applies a reduced palt to punctuation. Notable
/// exclusion: U+30FB (・) is punctuation, not a letter.
///
/// Block ranges:
/// - `0x3041..=0x3096` Hiragana letters (ぁ-ゖ)
/// - `0x3099..=0x309F` Hiragana combining/iteration marks
/// - `0x30A1..=0x30FA` Katakana letters (ァ-ヺ); excludes U+30FB (・)
/// - `0x30FC..=0x30FF` Katakana prolonged sound / iteration marks
/// - `0x31F0..=0x31FF` Katakana Phonetic Extensions
pub fn is_kana_letter(glyph_name: &str) -> bool {
    let Some(cp) = glyph_codepoint(glyph_name) else {
        return false;
    };
    (0x3041..=0x3096).contains(&cp)
        || (0x3099..=0x309F).contains(&cp)
        || (0x30A1..=0x30FA).contains(&cp)
        || (0x30FC..=0x30FF).contains(&cp)
        || (0x31F0..=0x31FF).contains(&cp)
}

/// Return `true` for CJK ideograph / radical / compatibility codepoints.
///
/// These are the glyphs we keep at full-width metrics — palt's narrowing
/// is for kana/punctuation rhythm, but a Han ideograph squeezed below
/// full-width loses its grid alignment with surrounding kanji. The block
/// list mirrors what Adobe and Google Noto treat as "ideographic" for
/// the purposes of full-width preservation.
///
/// Block ranges:
/// - `0x2E80..=0x2EFF` CJK Radicals Supplement
/// - `0x2F00..=0x2FDF` Kangxi Radicals
/// - `0x3020..=0x3029` Hangzhou-style numerals (〇〡〢…)
/// - `0x3038..=0x303B` CJK Symbols: 〸〹〺〻
/// - `0x3100..=0x312F` Bopomofo
/// - `0x3130..=0x318F` Hangul Compatibility Jamo
/// - `0x3190..=0x319F` Kanbun
/// - `0x31A0..=0x31EF` Bopomofo Extended + CJK Strokes
/// - `0x3200..=0x32FF` Enclosed CJK Letters and Months
/// - `0x3300..=0x33FF` CJK Compatibility
/// - `0x3400..=0x4DBF` CJK Unified Ideographs Extension A
/// - `0x4E00..=0x9FFF` CJK Unified Ideographs
/// - `0xF900..=0xFAFF` CJK Compatibility Ideographs
/// - `0x20000..=0x2FA1F` CJK Extensions B–F + Supplements
pub fn is_cjk_codepoint(cp: u32) -> bool {
    (0x2E80..=0x2EFF).contains(&cp)
        || (0x2F00..=0x2FDF).contains(&cp)
        || (0x3020..=0x3029).contains(&cp)
        || (0x3038..=0x303B).contains(&cp)
        || (0x3100..=0x312F).contains(&cp)
        || (0x3130..=0x318F).contains(&cp)
        || (0x3190..=0x319F).contains(&cp)
        || (0x31A0..=0x31EF).contains(&cp)
        || (0x3200..=0x32FF).contains(&cp)
        || (0x3300..=0x33FF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x4E00..=0x9FFF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0x20000..=0x2FA1F).contains(&cp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_codepoint_basic() {
        assert_eq!(glyph_codepoint("uni3042"), Some(0x3042));
    }

    #[test]
    fn glyph_codepoint_tolerant_suffix() {
        // Noto sometimes ships `uni3042.alt`-style names; tolerant parse.
        assert_eq!(glyph_codepoint("uni3042.alt"), Some(0x3042));
    }

    #[test]
    fn glyph_codepoint_non_uni_returns_none() {
        assert_eq!(glyph_codepoint("A"), None);
    }

    #[test]
    fn glyph_codepoint_short_returns_none() {
        // Fewer than 4 hex digits after `uni` — bail rather than misparse.
        assert_eq!(glyph_codepoint("uni30"), None);
    }

    #[test]
    fn glyph_codepoint_non_hex_returns_none() {
        assert_eq!(glyph_codepoint("uniZZZZ"), None);
    }

    #[test]
    fn is_kana_or_punct_hiragana() {
        assert!(is_kana_or_punct("uni3042")); // あ
    }

    #[test]
    fn is_kana_or_punct_latin_false() {
        assert!(!is_kana_or_punct("A"));
    }

    #[test]
    fn is_kana_or_punct_middle_dot() {
        // U+30FB (・) is punctuation — included by the lax classifier.
        assert!(is_kana_or_punct("uni30FB"));
    }

    #[test]
    fn is_kana_letter_excludes_middle_dot() {
        // Boundary case: 30FB is the gap between katakana letter ranges.
        assert!(!is_kana_letter("uni30FB"));
    }

    #[test]
    fn is_kana_letter_includes_30fa() {
        assert!(is_kana_letter("uni30FA")); // ヺ
    }

    #[test]
    fn is_kana_letter_includes_hiragana() {
        assert!(is_kana_letter("uni3042")); // あ
    }

    #[test]
    fn is_kana_letter_excludes_latin() {
        assert!(!is_kana_letter("A"));
    }

    #[test]
    fn is_cjk_codepoint_han() {
        assert!(is_cjk_codepoint(0x4E00)); // 一
    }

    #[test]
    fn is_cjk_codepoint_hiragana_false() {
        assert!(!is_cjk_codepoint(0x3042)); // あ — kana, not ideograph
    }

    #[test]
    fn is_cjk_codepoint_extension_b() {
        assert!(is_cjk_codepoint(0x20000));
        assert!(is_cjk_codepoint(0x2FA1F));
    }

    #[test]
    fn is_cjk_codepoint_out_of_range() {
        assert!(!is_cjk_codepoint(0x0041)); // 'A'
        assert!(!is_cjk_codepoint(0x2FA20)); // just past upper bound
    }
}
