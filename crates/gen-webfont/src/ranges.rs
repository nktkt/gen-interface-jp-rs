use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Latin core + symbols. Mirrors Google Fonts' base Latin slice.
pub const LATIN_RANGES: &[(u32, u32)] = &[
    (0x0000, 0x00FF),
    (0x0131, 0x0131),
    (0x0152, 0x0153),
    (0x02BB, 0x02BC),
    (0x02C6, 0x02C6),
    (0x02DA, 0x02DA),
    (0x02DC, 0x02DC),
    (0x0304, 0x0304),
    (0x0308, 0x0308),
    (0x0329, 0x0329),
    (0x2000, 0x206F),
    (0x20AC, 0x20AC),
    (0x2122, 0x2122),
    (0x2191, 0x2193),
    (0x2212, 0x2215),
    (0xFEFF, 0xFEFF),
    (0xFFFD, 0xFFFD),
];

pub const JP_KANA_RANGES: &[(u32, u32)] = &[
    (0x3000, 0x303F),
    (0x3040, 0x309F),
    (0x30A0, 0x30FF),
    (0x31F0, 0x31FF),
    (0xFF00, 0xFFEF),
];

pub const JP_SYMBOL_RANGES: &[(u32, u32)] = &[
    (0x2E80, 0x2EFF),
    (0x2F00, 0x2FDF),
    (0x3100, 0x312F),
    (0x3190, 0x319F),
    (0x3200, 0x32FF),
    (0x3300, 0x33FF),
];

/// Expand `[(start, end), ...]` ranges (inclusive) into a sorted set of codepoints.
pub fn codepoints_from_ranges(ranges: &[(u32, u32)]) -> BTreeSet<u32> {
    let mut cps: BTreeSet<u32> = BTreeSet::new();
    for &(start, end) in ranges {
        for cp in start..=end {
            cps.insert(cp);
        }
    }
    cps
}

/// True for CJK ideograph blocks (Han only — used for JIS-row "extra Han" bucket).
/// Ranges: 0x3400..=0x4DBF, 0x4E00..=0x9FFF, 0xF900..=0xFAFF, 0x20000..=0x2FA1F.
pub fn is_han_codepoint(cp: u32) -> bool {
    (0x3400..=0x4DBF).contains(&cp)
        || (0x4E00..=0x9FFF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0x20000..=0x2FA1F).contains(&cp)
}

/// Collapse a set of codepoints into contiguous (start, end) inclusive ranges.
pub fn merge_codepoints_to_ranges<I: IntoIterator<Item = u32>>(cps: I) -> Vec<(u32, u32)> {
    let values: BTreeSet<u32> = cps.into_iter().collect();
    if values.is_empty() {
        return Vec::new();
    }
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    let mut iter = values.into_iter();
    let first = iter.next().unwrap();
    let mut start = first;
    let mut prev = first;
    for cp in iter {
        if cp == prev + 1 {
            prev = cp;
            continue;
        }
        ranges.push((start, prev));
        start = cp;
        prev = cp;
    }
    ranges.push((start, prev));
    ranges
}

fn format_codepoint(cp: u32) -> String {
    if cp > 0xFFFF {
        format!("{:X}", cp)
    } else {
        format!("{:04X}", cp)
    }
}

/// Format a list of codepoints as a CSS unicode-range value.
/// Single codepoints render as `U+0041`, ranges render as `U+0041-005A`,
/// joined by `, `.
pub fn format_unicode_range<I: IntoIterator<Item = u32>>(cps: I) -> String {
    let ranges = merge_codepoints_to_ranges(cps);
    let mut out = String::new();
    for (i, (start, end)) in ranges.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if start == end {
            let _ = write!(out, "U+{}", format_codepoint(*start));
        } else {
            let _ = write!(
                out,
                "U+{}-{}",
                format_codepoint(*start),
                format_codepoint(*end)
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_contiguous() {
        assert_eq!(
            merge_codepoints_to_ranges([0x41, 0x42, 0x43]),
            vec![(0x41, 0x43)]
        );
    }

    #[test]
    fn merge_split() {
        assert_eq!(
            merge_codepoints_to_ranges([0x41, 0x43]),
            vec![(0x41, 0x41), (0x43, 0x43)]
        );
    }

    #[test]
    fn format_single() {
        assert_eq!(format_unicode_range([0x41]), "U+0041");
    }

    #[test]
    fn format_contiguous() {
        assert_eq!(format_unicode_range([0x41, 0x42, 0x43]), "U+0041-0043");
    }

    #[test]
    fn format_split() {
        assert_eq!(format_unicode_range([0x41, 0x43]), "U+0041, U+0043");
    }

    #[test]
    fn format_high_codepoint() {
        assert_eq!(format_unicode_range([0x1F130]), "U+1F130");
    }

    #[test]
    fn han_codepoint() {
        assert!(is_han_codepoint(0x4E00));
    }

    #[test]
    fn codepoints_from_ranges_length() {
        assert_eq!(codepoints_from_ranges(&[(0x41, 0x42)]).len(), 2);
    }
}
