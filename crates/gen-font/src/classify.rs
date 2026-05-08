//! Codepoint and glyph classification helpers.
//!
//! Ported from `source/src/font/build.py`. These helpers drive the
//! tracking and proportional-metrics passes: kana and CJK punctuation
//! read at a wider rhythm than Latin, ideographs must stay on the
//! full-width grid, and only kana *letters* (not the middle dot) want
//! the full palt shrink.

use std::collections::BTreeSet;

use read_fonts::{
    tables::gsub::{ExtensionSubtable, SingleSubst, SubstitutionLookup},
    types::Tag,
    TableProvider,
};
use skrifa::{charmap::Charmap, FontRef, GlyphId, GlyphNames};

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

/// Return the full ordered list of glyph names for `font`, indexed by glyph
/// id (`names[gid] = name`). Names that the font's `post`/CFF tables don't
/// expose fall back to skrifa's synthesised `gidNNN` placeholder.
///
/// Mirrors fontTools' `font.getGlyphOrder()` so the bucket-building logic in
/// [`crate::build::build_one`] can drive set operations by glyph name exactly
/// like the Python reference does.
pub fn glyph_names(font: &FontRef<'_>) -> Vec<String> {
    let glyph_names = GlyphNames::new(font);
    let Ok(maxp) = font.maxp() else {
        return Vec::new();
    };
    let num_glyphs = u32::from(maxp.num_glyphs());
    (0..num_glyphs)
        .map(|gid| {
            let glyph_id = GlyphId::new(gid);
            match glyph_names.get(glyph_id) {
                Some(n) => n.to_string(),
                None => format!("gid{gid}"),
            }
        })
        .collect()
}

/// Return glyph names that appear as substitutes in `vert` / `vrt2` GSUB
/// lookups — the rotated / vertical-form variants OpenType picks up under
/// vertical writing mode.
///
/// We collect them so the proportional pass and the bbox-strip pass can
/// avoid touching them: vertical-only glyphs don't contribute to the
/// horizontal rhythm we're tuning, and rewriting their metrics would
/// mismatch what the unrotated original expects.
///
/// Only single-substitution lookups are walked (mirrors the Python
/// `hasattr(st, "mapping")` filter — vertical lookups in Noto are
/// exclusively single-subs in practice). Extension lookups wrapping a
/// `SingleSubst` are unwrapped.
///
/// Port of `_get_vert_alternates` from `source/src/font/build.py` (lines
/// 210-233).
pub fn get_vert_alternates(font: &FontRef<'_>) -> anyhow::Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();

    // Missing GSUB / FeatureList / LookupList all surface as "no vertical
    // alternates". Python returns an empty set in the same shape, and callers
    // treat absence as "no glyphs to skip in the proportional pass".
    let Ok(gsub) = font.gsub() else {
        return Ok(out);
    };
    let Ok(feature_list) = gsub.feature_list() else {
        return Ok(out);
    };
    let Ok(lookup_list) = gsub.lookup_list() else {
        return Ok(out);
    };

    let vert_tag = Tag::new(b"vert");
    let vrt2_tag = Tag::new(b"vrt2");

    let feature_data = feature_list.offset_data();
    let mut lookup_indices: Vec<u16> = Vec::new();
    for record in feature_list.feature_records() {
        let tag = record.feature_tag();
        if tag != vert_tag && tag != vrt2_tag {
            continue;
        }
        let Ok(feature) = record.feature(feature_data) else {
            continue;
        };
        for raw in feature.lookup_list_indices() {
            lookup_indices.push(raw.get());
        }
    }
    if lookup_indices.is_empty() {
        return Ok(out);
    }

    let glyph_names_resolver = GlyphNames::new(font);
    let lookups = lookup_list.lookups();
    for index in lookup_indices {
        let Ok(lookup) = lookups.get(index as usize) else {
            continue;
        };
        match lookup {
            SubstitutionLookup::Single(single) => {
                for sub in single.subtables().iter().flatten() {
                    record_single_subst_targets(&sub, &glyph_names_resolver, &mut out);
                }
            }
            SubstitutionLookup::Extension(ext) => {
                for sub in ext.subtables().iter().flatten() {
                    if let ExtensionSubtable::Single(ext_single) = sub {
                        if let Ok(inner) = ext_single.extension() {
                            record_single_subst_targets(&inner, &glyph_names_resolver, &mut out);
                        }
                    }
                }
            }
            // Multi / ligature / contextual / chained — Python's `hasattr(st,
            // "mapping")` skips them, mirror that here.
            _ => {}
        }
    }
    Ok(out)
}

/// Add every substitute glyph name from a `SingleSubst` subtable to `out`.
fn record_single_subst_targets(
    subst: &SingleSubst<'_>,
    glyph_names_resolver: &GlyphNames<'_>,
    out: &mut BTreeSet<String>,
) {
    match subst {
        SingleSubst::Format1(fmt1) => {
            // Format 1: substitute = (key + delta) mod 65536 for every
            // glyph in coverage.
            let Ok(coverage) = fmt1.coverage() else {
                return;
            };
            let delta = fmt1.delta_glyph_id() as i32;
            for gid16 in coverage.iter() {
                let key = i32::from(gid16.to_u16());
                let sub_gid = ((key + delta).rem_euclid(0x10000)) as u32;
                if let Some(name) = glyph_names_resolver.get(GlyphId::new(sub_gid)) {
                    out.insert(name.to_string());
                }
            }
        }
        SingleSubst::Format2(fmt2) => {
            // Format 2: explicit per-glyph substitutes; `substitute_glyph_ids`
            // is parallel to coverage order.
            for gid16 in fmt2.substitute_glyph_ids() {
                let sub_gid = u32::from(gid16.get().to_u16());
                if let Some(name) = glyph_names_resolver.get(GlyphId::new(sub_gid)) {
                    out.insert(name.to_string());
                }
            }
        }
    }
}

/// Return the set of glyph IDs whose codepoint is in the kana / CJK punctuation
/// blocks. Uses the font's cmap directly (more robust than glyph-name parsing,
/// which only works for fonts whose post table exposes Adobe-style `uniXXXX`
/// names).
///
/// Codepoint ranges checked (matching `is_kana_or_punct` by codepoint):
///   - 0x3000..=0x303F (CJK Symbols and Punctuation)
///   - 0x3040..=0x309F (Hiragana)
///   - 0x30A0..=0x30FF (Katakana)
///   - 0x31F0..=0x31FF (Katakana Phonetic Extensions)
///   - 0xFF00..=0xFFEF (Halfwidth and Fullwidth Forms)
pub fn get_kana_or_punct_glyphs(font: &FontRef<'_>) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    let charmap = Charmap::new(font);
    for (cp, gid) in charmap.mappings() {
        if (0x3000..=0x303F).contains(&cp)
            || (0x3040..=0x309F).contains(&cp)
            || (0x30A0..=0x30FF).contains(&cp)
            || (0x31F0..=0x31FF).contains(&cp)
            || (0xFF00..=0xFFEF).contains(&cp)
        {
            out.insert(gid.to_u32());
        }
    }
    out
}

/// Resolve CJK ideograph glyph names through the font's cmap.
///
/// Cmap-driven (rather than glyph-name parsing) so that ideographs whose names
/// don't follow `uniXXXX` are still caught — Noto ships some Han glyphs as
/// `cidNNNNN` or post-substitution names that wouldn't match a `uni`-prefix
/// check.
///
/// Port of `_get_cjk_glyphs` from `source/src/font/build.py` (lines 267-278).
pub fn get_cjk_glyphs(font: &FontRef<'_>) -> anyhow::Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    let charmap = Charmap::new(font);
    let glyph_names_resolver = GlyphNames::new(font);
    for (cp, gid) in charmap.mappings() {
        if !is_cjk_codepoint(cp) {
            continue;
        }
        if let Some(name) = glyph_names_resolver.get(gid) {
            out.insert(name.to_string());
        }
    }
    Ok(out)
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
