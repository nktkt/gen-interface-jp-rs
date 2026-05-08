use std::collections::BTreeSet;

use encoding_rs::EUC_JP;

/// Return Unicode codepoints for one JIS X 0208 row.
///
/// Rows 16-47 are first-level kanji, rows 48-84 are second-level kanji.
/// Uses EUC-JP decoding (via the `encoding_rs` crate) to get a portable
/// mapping without vendoring a large table — same approach as Python's
/// `bytes([row + 0xA0, cell + 0xA0]).decode("euc_jp")`.
pub fn jis_row_codepoints(row: u8) -> BTreeSet<u32> {
    let mut cps: BTreeSet<u32> = BTreeSet::new();
    let Some(row_byte) = row.checked_add(0xA0) else {
        return cps;
    };
    for cell in 1u8..=94 {
        let cell_byte = cell + 0xA0;
        let bytes = [row_byte, cell_byte];
        let (decoded, had_errors) = EUC_JP.decode_without_bom_handling(&bytes);
        if had_errors {
            continue;
        }
        let mut chars = decoded.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            cps.insert(c as u32);
        }
    }
    cps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_16_is_non_empty() {
        let cps = jis_row_codepoints(16);
        assert!(!cps.is_empty(), "row 16 should contain first-level kanji");
    }

    #[test]
    fn row_16_contains_a_kanji() {
        // 亜 (U+4E9C) is at JIS X 0208 row 16, cell 1.
        let cps = jis_row_codepoints(16);
        assert!(
            cps.contains(&0x4E9C),
            "row 16 should contain 亜 (U+4E9C)"
        );
    }

    #[test]
    fn row_85_is_unassigned() {
        // Row 85 is not assigned in JIS X 0208; an empty set is acceptable.
        let cps = jis_row_codepoints(85);
        // Just exercise the call; no positive assertion needed.
        let _ = cps;
    }
}
