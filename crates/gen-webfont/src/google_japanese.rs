//! Google Fonts' Japanese slicing strategy.
//!
//! This is a Rust port of `parse_slicing_strategy` and
//! `build_google_japanese_subset_plan` from `source/src/webfont/build.py`.
//!
//! The strategy textproto comes from `googlefonts/nam-files`. We parse it
//! ourselves rather than pull in a full protobuf stack: only `codepoints:`
//! lines are interpreted, so the parser is a tiny line-oriented affair.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Result};
use regex::Regex;

use crate::plan::WebFontSubset;

/// Parse a googlefonts/nam-files slicing strategy textproto.
///
/// Format:
/// ```text
/// subsets {
///   codepoints: 12354 # あ
///   codepoints: 0x3042
///   ...
/// }
/// ```
/// Only `codepoints:` lines are interpreted, so comment text can contain
/// braces like `# } RIGHT CURLY BRACKET`.
pub fn parse_slicing_strategy(path: &Path) -> Result<Vec<BTreeSet<u32>>> {
    let text = fs::read_to_string(path)
        .map_err(|e| anyhow!("failed to read slicing strategy {}: {}", path.display(), e))?;

    // `subsets\s*\{` must match the entire stripped line — like Python's
    // `re.fullmatch`. We anchor the regex with `\A` and `\z`.
    let open_re = Regex::new(r"\Asubsets\s*\{\z").expect("static regex");
    // `re.match` in Python anchors at the start only.
    let cp_re = Regex::new(r"\Acodepoints:\s*(0x[0-9A-Fa-f]+|\d+)").expect("static regex");

    let mut parsed: Vec<BTreeSet<u32>> = Vec::new();
    let mut current: Option<BTreeSet<u32>> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();

        if open_re.is_match(line) {
            if current.is_some() {
                return Err(anyhow!(
                    "Nested subsets block in slicing strategy: {}",
                    path.display()
                ));
            }
            current = Some(BTreeSet::new());
            continue;
        }

        if line == "}" {
            if let Some(set) = current.take() {
                if !set.is_empty() {
                    parsed.push(set);
                }
            }
            continue;
        }

        let Some(set) = current.as_mut() else {
            continue;
        };

        if let Some(caps) = cp_re.captures(line) {
            let raw = caps.get(1).expect("captured group 1").as_str();
            let value = parse_int_auto(raw).ok_or_else(|| {
                anyhow!(
                    "Invalid codepoint literal '{}' in slicing strategy: {}",
                    raw,
                    path.display()
                )
            })?;
            set.insert(value);
        }
    }

    if current.is_some() {
        return Err(anyhow!(
            "Unclosed subsets block in slicing strategy: {}",
            path.display()
        ));
    }
    if parsed.is_empty() {
        return Err(anyhow!(
            "No subsets were found in slicing strategy: {}",
            path.display()
        ));
    }

    Ok(parsed)
}

/// Parse an integer literal in either base 16 (with `0x` prefix) or base 10,
/// matching Python's `int(s, 0)` auto-detection for the slice in question.
fn parse_int_auto(s: &str) -> Option<u32> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

/// Split `values` (already sorted) into approximately `chunks` even pieces,
/// matching Python's `_chunk_evenly`.
fn chunk_evenly(values: &[u32], chunks: usize) -> Vec<Vec<u32>> {
    if values.is_empty() {
        return Vec::new();
    }
    // Guard against `chunks == 0` to avoid a division by zero — Python relies
    // on the caller passing a positive value, but we should not panic.
    let chunks = chunks.max(1);
    let chunk_size = ((values.len() + chunks - 1) / chunks).max(1);
    values
        .chunks(chunk_size)
        .map(|c| c.to_vec())
        .collect()
}

/// Build subsets from googlefonts/nam-files' Japanese slicing strategy.
///
/// The strategy file is ordered the same way as Google Fonts' unicode-range
/// prioritisation. We preserve that order, intersect each slice with the
/// font's cmap, then optionally add any cmap codepoints not covered by the
/// Japanese strategy so the self-hosted build can still serve the full font.
pub fn build_google_japanese_subset_plan<I: IntoIterator<Item = u32>>(
    font_codepoints: I,
    slice_path: &Path,
    include_remaining: bool,
    remaining_slices: usize,
) -> Result<Vec<WebFontSubset>> {
    let supported: BTreeSet<u32> = font_codepoints.into_iter().collect();
    let mut assigned: BTreeSet<u32> = BTreeSet::new();
    let mut subsets: Vec<WebFontSubset> = Vec::new();

    let slices = parse_slicing_strategy(slice_path)?;

    for (index, codepoints) in slices.into_iter().enumerate() {
        // Sorted intersection minus already-assigned. BTreeSet iterates in
        // sorted order, so collecting into Vec preserves sort order — matching
        // the Python `tuple(sorted(...))`.
        let usable: Vec<u32> = codepoints
            .iter()
            .filter(|cp| supported.contains(cp) && !assigned.contains(cp))
            .copied()
            .collect();
        if usable.is_empty() {
            continue;
        }
        let name = format!("google-japanese-{:03}", index);
        let note = format!(
            "googlefonts/nam-files slices/japanese_default.txt subset {}",
            index
        );
        assigned.extend(usable.iter().copied());
        subsets.push(WebFontSubset {
            name,
            codepoints: usable,
            note,
        });
    }

    if include_remaining {
        let remaining: Vec<u32> = supported.difference(&assigned).copied().collect();
        for (index, chunk) in chunk_evenly(&remaining, remaining_slices).into_iter().enumerate() {
            subsets.push(WebFontSubset {
                name: format!("google-japanese-extra-{:02}", index),
                codepoints: chunk,
                note:
                    "Codepoints supported by Gen Interface JP but not covered by \
                     googlefonts/nam-files Japanese slicing strategy"
                        .to_string(),
            });
        }
    }

    Ok(subsets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: write `body` to a temp file and return the path. The path is
    /// leaked because the file is small and the test process is short-lived.
    fn write_temp(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("gen-webfont-test-{}-{}.txt", name, std::process::id()));
        let mut f = std::fs::File::create(&path).expect("create temp file");
        f.write_all(body.as_bytes()).expect("write temp file");
        path
    }

    #[test]
    fn parses_two_subsets() {
        let body = "\
subsets {
  codepoints: 0x41
  codepoints: 0x42
}
subsets {
  codepoints: 0x4E00
}
";
        let path = write_temp("two-subsets", body);
        let parsed = parse_slicing_strategy(&path).expect("parse ok");
        assert_eq!(parsed.len(), 2);

        let first: BTreeSet<u32> = [0x41u32, 0x42].into_iter().collect();
        let second: BTreeSet<u32> = [0x4E00u32].into_iter().collect();
        assert_eq!(parsed[0], first);
        assert_eq!(parsed[1], second);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parses_decimal_and_hex_codepoints() {
        let body = "\
subsets {
  codepoints: 12354
  codepoints: 0x3042
}
";
        let path = write_temp("dec-hex", body);
        let parsed = parse_slicing_strategy(&path).expect("parse ok");
        assert_eq!(parsed.len(), 1);
        // 12354 == 0x3042; the duplicate collapses inside the BTreeSet.
        let want: BTreeSet<u32> = [0x3042u32].into_iter().collect();
        assert_eq!(parsed[0], want);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn comment_with_closing_brace_does_not_close_block() {
        // A comment containing `}` must not be mistaken for the block close.
        // The line as a whole is not equal to `"}"` after `.trim()`, so the
        // parser keeps the block open and consumes the codepoint.
        let body = "\
subsets {
  codepoints: 0x7D # } RIGHT CURLY BRACKET
}
";
        let path = write_temp("brace-comment", body);
        let parsed = parse_slicing_strategy(&path).expect("parse ok");
        assert_eq!(parsed.len(), 1);
        let want: BTreeSet<u32> = [0x7Du32].into_iter().collect();
        assert_eq!(parsed[0], want);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn empty_subsets_are_discarded() {
        let body = "\
subsets {
}
subsets {
  codepoints: 0x41
}
subsets {
}
";
        let path = write_temp("empty-discarded", body);
        let parsed = parse_slicing_strategy(&path).expect("parse ok");
        assert_eq!(parsed.len(), 1);
        let want: BTreeSet<u32> = [0x41u32].into_iter().collect();
        assert_eq!(parsed[0], want);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn nested_block_is_an_error() {
        let body = "\
subsets {
  subsets {
  }
}
";
        let path = write_temp("nested", body);
        let err = parse_slicing_strategy(&path).expect_err("nested should fail");
        assert!(err.to_string().contains("Nested"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unclosed_block_is_an_error() {
        let body = "\
subsets {
  codepoints: 0x41
";
        let path = write_temp("unclosed", body);
        let err = parse_slicing_strategy(&path).expect_err("unclosed should fail");
        assert!(err.to_string().contains("Unclosed"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn no_subsets_is_an_error() {
        let body = "# just a comment\n";
        let path = write_temp("no-subsets", body);
        let err = parse_slicing_strategy(&path).expect_err("no subsets should fail");
        assert!(err.to_string().contains("No subsets"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn build_plan_intersects_with_supported_and_dedupes() {
        // Two slices, with overlap between them. The font supports a subset of
        // each. We expect:
        //  - subset 0 contains the supported intersection of slice 0
        //  - subset 1 contains slice 1 minus what subset 0 already claimed
        //  - extras cover supported codepoints absent from both slices
        let body = "\
subsets {
  codepoints: 0x41
  codepoints: 0x42
  codepoints: 0x43
}
subsets {
  codepoints: 0x43
  codepoints: 0x44
}
";
        let path = write_temp("build-plan", body);
        let font_cps = [0x41u32, 0x42, 0x43, 0x44, 0x99];
        let plan = build_google_japanese_subset_plan(font_cps, &path, true, 2)
            .expect("build plan ok");

        // First two entries from the slice file.
        assert_eq!(plan[0].name, "google-japanese-000");
        assert_eq!(plan[0].codepoints, vec![0x41, 0x42, 0x43]);
        assert_eq!(plan[1].name, "google-japanese-001");
        assert_eq!(plan[1].codepoints, vec![0x44]);

        // Plus an extras chunk for the unassigned 0x99.
        assert!(plan.iter().any(|s| s.name == "google-japanese-extra-00"
            && s.codepoints == vec![0x99]));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn build_plan_skips_remaining_when_disabled() {
        let body = "\
subsets {
  codepoints: 0x41
}
";
        let path = write_temp("no-remaining", body);
        let plan = build_google_japanese_subset_plan([0x41u32, 0x99], &path, false, 4)
            .expect("build plan ok");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].name, "google-japanese-000");
        let _ = std::fs::remove_file(path);
    }
}
