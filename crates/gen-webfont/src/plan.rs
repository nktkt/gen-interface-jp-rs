use std::collections::BTreeSet;

use crate::jis::jis_row_codepoints;
use crate::ranges::{
    codepoints_from_ranges, is_han_codepoint, JP_KANA_RANGES, JP_SYMBOL_RANGES, LATIN_RANGES,
};

/// Default `extra_han_slices` value matching the Python.
pub const DEFAULT_EXTRA_HAN_SLICES: usize = 24;

#[derive(Debug, Clone)]
pub struct WebFontSubset {
    pub name: String,
    pub codepoints: Vec<u32>,
    pub note: String,
}

/// Mirror Python's `_chunk_evenly`:
/// `chunk_size = max(1, ceil(len/chunks))`, then split into contiguous slices.
fn chunk_evenly<T: Clone>(values: &[T], chunks: usize) -> Vec<Vec<T>> {
    if values.is_empty() {
        return Vec::new();
    }
    let len = values.len();
    let denom = chunks.max(1);
    // ceil(len / denom)
    let chunk_size = len.div_ceil(denom).max(1);
    let mut out: Vec<Vec<T>> = Vec::new();
    let mut i = 0;
    while i < len {
        let end = (i + chunk_size).min(len);
        out.push(values[i..end].to_vec());
        i = end;
    }
    out
}

/// Build non-overlapping subsets from the font cmap (jis-row strategy).
///
/// Order: latin → jp-kana → jp-symbols → JIS row 16-47 (jp-kanji-jis1-NN)
///        → JIS row 48-84 (jp-kanji-jis2-NN) → CJK extra (chunked into
///        `extra_han_slices`) → other (chunked into 8).
///
/// Each bucket subtracts already-assigned codepoints, so the buckets are
/// disjoint. Empty buckets are skipped silently.
pub fn build_subset_plan<I: IntoIterator<Item = u32>>(
    font_codepoints: I,
    extra_han_slices: usize,
) -> Vec<WebFontSubset> {
    let supported: BTreeSet<u32> = font_codepoints.into_iter().collect();
    let mut assigned: BTreeSet<u32> = BTreeSet::new();
    let mut subsets: Vec<WebFontSubset> = Vec::new();

    // Mirrors the Python `add(name, codepoints, note)` closure: intersect with
    // `supported`, subtract `assigned`, push if non-empty, then update `assigned`.
    fn add(
        supported: &BTreeSet<u32>,
        assigned: &mut BTreeSet<u32>,
        subsets: &mut Vec<WebFontSubset>,
        name: &str,
        cps: &BTreeSet<u32>,
        note: &str,
    ) {
        let usable: Vec<u32> = cps
            .iter()
            .copied()
            .filter(|cp| supported.contains(cp) && !assigned.contains(cp))
            .collect();
        if usable.is_empty() {
            return;
        }
        for cp in &usable {
            assigned.insert(*cp);
        }
        subsets.push(WebFontSubset {
            name: name.to_string(),
            codepoints: usable,
            note: note.to_string(),
        });
    }

    add(
        &supported,
        &mut assigned,
        &mut subsets,
        "latin",
        &codepoints_from_ranges(LATIN_RANGES),
        "Latin, Latin punctuation, and shared symbols",
    );
    add(
        &supported,
        &mut assigned,
        &mut subsets,
        "jp-kana",
        &codepoints_from_ranges(JP_KANA_RANGES),
        "Japanese punctuation, kana, and fullwidth forms",
    );
    add(
        &supported,
        &mut assigned,
        &mut subsets,
        "jp-symbols",
        &codepoints_from_ranges(JP_SYMBOL_RANGES),
        "Japanese radicals, enclosed forms, and CJK symbols",
    );

    for row in 16u8..48 {
        let name = format!("jp-kanji-jis1-{row:02}");
        let note = format!("JIS X 0208 first-level kanji row {row}");
        add(
            &supported,
            &mut assigned,
            &mut subsets,
            &name,
            &jis_row_codepoints(row),
            &note,
        );
    }

    for row in 48u8..85 {
        let name = format!("jp-kanji-jis2-{row:02}");
        let note = format!("JIS X 0208 second-level kanji row {row}");
        add(
            &supported,
            &mut assigned,
            &mut subsets,
            &name,
            &jis_row_codepoints(row),
            &note,
        );
    }

    let remaining_han: Vec<u32> = supported
        .iter()
        .copied()
        .filter(|cp| !assigned.contains(cp) && is_han_codepoint(*cp))
        .collect();
    for (index, chunk) in chunk_evenly(&remaining_han, extra_han_slices)
        .into_iter()
        .enumerate()
    {
        let name = format!("jp-kanji-extra-{index:02}");
        let chunk_set: BTreeSet<u32> = chunk.into_iter().collect();
        add(
            &supported,
            &mut assigned,
            &mut subsets,
            &name,
            &chunk_set,
            "CJK codepoints outside JIS X 0208 rows",
        );
    }

    let remaining: Vec<u32> = supported
        .iter()
        .copied()
        .filter(|cp| !assigned.contains(cp))
        .collect();
    for (index, chunk) in chunk_evenly(&remaining, 8).into_iter().enumerate() {
        let name = format!("other-{index:02}");
        let chunk_set: BTreeSet<u32> = chunk.into_iter().collect();
        add(
            &supported,
            &mut assigned,
            &mut subsets,
            &name,
            &chunk_set,
            "Non-Japanese fallback coverage",
        );
    }

    subsets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn chunk_evenly_two_chunks() {
        let chunks = chunk_evenly(&[1, 2, 3, 4, 5], 2);
        assert_eq!(chunks.len(), 2);
        let total: usize = chunks.iter().map(std::vec::Vec::len).sum();
        assert_eq!(total, 5);
    }

    #[test]
    fn chunk_evenly_empty() {
        let chunks: Vec<Vec<i32>> = chunk_evenly::<i32>(&[], 4);
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_evenly_zero_chunks_treated_as_one() {
        // Python's max(1, ceil(...)) means 0 chunks behaves like 1 (avoids div-by-zero).
        let chunks = chunk_evenly(&[1, 2, 3], 0);
        let total: usize = chunks.iter().map(std::vec::Vec::len).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn build_subset_plan_no_overlap_and_full_coverage() {
        let input: Vec<u32> = vec![0x41, 0x42, 0x3042];
        let plan = build_subset_plan(input.iter().copied(), DEFAULT_EXTRA_HAN_SLICES);

        let mut all: Vec<u32> = Vec::new();
        let mut seen: HashSet<u32> = HashSet::new();
        for subset in &plan {
            for cp in &subset.codepoints {
                assert!(
                    seen.insert(*cp),
                    "duplicate codepoint across subsets: {cp:#x}"
                );
                all.push(*cp);
            }
        }

        let input_set: HashSet<u32> = input.iter().copied().collect();
        assert_eq!(seen, input_set);
        assert_eq!(all.len(), input.len());
    }

    #[test]
    fn first_subset_is_latin_when_input_includes_latin() {
        let input: Vec<u32> = vec![0x41, 0x42, 0x3042];
        let plan = build_subset_plan(input.iter().copied(), DEFAULT_EXTRA_HAN_SLICES);
        assert!(!plan.is_empty());
        assert_eq!(plan[0].name, "latin");
    }
}
