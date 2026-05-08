//! Manifest building plus brotli/gzip size helpers.
//!
//! Ports the size helpers (`_brotli_size`, `css_size_info`) and the
//! `manifest` dict construction in `build_all` / `build` from
//! `source/src/webfont/build.py`. The JSON shape produced here must match
//! what the Python script emits so downstream consumers (npm package,
//! GitHub Pages mirror, `benchmark.mjs`) can read it unchanged.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use brotli::enc::BrotliEncoderParams;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Size helpers
// ---------------------------------------------------------------------------

/// CSS file size info: raw bytes, gzip bytes, brotli bytes.
///
/// Mirrors the dict shape returned by Python `css_size_info`.
#[derive(Debug, Clone, Serialize)]
pub struct CssSizeInfo {
    pub path: String,
    pub bytes: usize,
    #[serde(rename = "gzipBytes")]
    pub gzip_bytes: usize,
    #[serde(rename = "brotliBytes")]
    pub brotli_bytes: Option<usize>,
}

/// Compute size info for a CSS file on disk: raw, gzip (level=9), brotli (quality=11).
pub fn css_size_info(path: &Path) -> anyhow::Result<CssSizeInfo> {
    let data = fs::read(path)?;
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(CssSizeInfo {
        path: file_name,
        bytes: data.len(),
        gzip_bytes: gzip_size(&data),
        brotli_bytes: brotli_size(&data),
    })
}

/// Compute brotli compressed size of `data` at quality=11.
///
/// Returns `None` only on encoder failure (the Python equivalent returns
/// `None` if the `brotli` package is missing — in Rust we have it as a hard
/// dep but keep the `Option` shape so the JSON matches).
pub fn brotli_size(data: &[u8]) -> Option<usize> {
    let mut out = Vec::with_capacity(data.len());
    let mut params = BrotliEncoderParams::default();
    params.quality = 11;
    let mut input = data;
    match brotli::BrotliCompress(&mut input, &mut out, &params) {
        Ok(_) => Some(out.len()),
        Err(_) => None,
    }
}

/// Compute gzip compressed size of `data` at compresslevel=9.
pub fn gzip_size(data: &[u8]) -> usize {
    let mut encoder = GzEncoder::new(Vec::with_capacity(data.len()), Compression::new(9));
    // Writes to an in-memory `Vec` cannot fail in practice; surface a 0
    // here would be misleading, so panic-on-OOM is acceptable.
    encoder
        .write_all(data)
        .expect("gzip write to Vec is infallible");
    let buf = encoder.finish().expect("gzip finish on Vec is infallible");
    buf.len()
}

// ---------------------------------------------------------------------------
// Timestamp
// ---------------------------------------------------------------------------

/// RFC 3339 / ISO 8601 UTC timestamp, matching Python's
/// `datetime.now(tz=timezone.utc).isoformat()` output shape
/// (`YYYY-MM-DDTHH:MM:SS.ssssss+00:00`).
fn iso_utc_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let micros = now.subsec_micros();

    // Civil-from-days algorithm (Howard Hinnant, public domain) to get
    // y/m/d from the unix epoch without pulling in chrono.
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400) as u32;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}.{micros:06}+00:00")
}

// ---------------------------------------------------------------------------
// Source descriptor (shared between both manifests)
// ---------------------------------------------------------------------------

/// `source` block of the manifest. Field set differs between the two flows:
/// `build_all` writes `format`, `build` writes `ttf`. `googleJapaneseSlice`
/// is only set when `strategy == "google-japanese"`.
#[derive(Debug, Serialize)]
pub struct ManifestSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttf: Option<String>,
    pub strategy: String,
    #[serde(
        rename = "googleJapaneseSlice",
        skip_serializing_if = "Option::is_none"
    )]
    pub google_japanese_slice: Option<String>,
}

// ---------------------------------------------------------------------------
// build_all manifest (multi-weight, subset-only)
// ---------------------------------------------------------------------------

/// One subset WOFF2 file entry in `files.subsets[]` for the `build_all` flow.
#[derive(Debug, Clone, Serialize)]
pub struct SubsetFileEntry {
    pub family: String,
    #[serde(rename = "familyKey")]
    pub family_key: String,
    pub weight: u16,
    pub subset: String,
    pub path: String,
    pub source: String,
    pub codepoints: usize,
    #[serde(rename = "unicodeRange")]
    pub unicode_range: String,
    pub bytes: u64,
}

/// `css` block of the `build_all` manifest.
#[derive(Debug, Serialize)]
pub struct CssEntries {
    pub all: CssSizeInfo,
    /// Maps weight (e.g. `"100"`, `"400"`) to its CSS size info. `BTreeMap`
    /// for stable insertion-style ordering by stringified weight; the Python
    /// version relies on dict insertion order from iterating `WEIGHTS`.
    pub weights: BTreeMap<String, CssSizeInfo>,
    #[serde(rename = "displayWeights")]
    pub display_weights: BTreeMap<String, CssSizeInfo>,
}

#[derive(Debug, Serialize)]
pub struct FamilyEntry {
    pub key: String,
    pub family: String,
    pub weights: Vec<u16>,
}

#[derive(Debug, Serialize)]
pub struct TotalsAll {
    #[serde(rename = "fontCodepoints")]
    pub font_codepoints: usize,
    #[serde(rename = "subsetCount")]
    pub subset_count: usize,
    #[serde(rename = "fontFaceCount")]
    pub font_face_count: usize,
    #[serde(rename = "subsetFileCount")]
    pub subset_file_count: usize,
    #[serde(rename = "subsetBytes")]
    pub subset_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct FilesAll {
    pub subsets: Vec<SubsetFileEntry>,
}

/// Manifest for the multi-weight `build_all` flow (one file per
/// (subset x family x weight) WOFF2 + per-weight + combined CSS).
#[derive(Debug, Serialize)]
pub struct ManifestAll {
    pub family: String,
    pub style: String,
    #[serde(rename = "fontDisplay")]
    pub font_display: String,
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    pub source: ManifestSource,
    pub css: CssEntries,
    pub families: Vec<FamilyEntry>,
    pub totals: TotalsAll,
    pub files: FilesAll,
}

impl ManifestAll {
    /// Construct a `ManifestAll`, computing `generatedAt` and `totals.subsetBytes`.
    /// Caller supplies the family/style/display strings and pre-built sub-blocks.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        family: impl Into<String>,
        style: impl Into<String>,
        font_display: impl Into<String>,
        source: ManifestSource,
        css: CssEntries,
        families: Vec<FamilyEntry>,
        font_codepoints: usize,
        subset_count: usize,
        font_face_count: usize,
        subsets: Vec<SubsetFileEntry>,
    ) -> Self {
        let subset_bytes: u64 = subsets.iter().map(|e| e.bytes).sum();
        let subset_file_count = subsets.len();
        Self {
            family: family.into(),
            style: style.into(),
            font_display: font_display.into(),
            generated_at: iso_utc_now(),
            source,
            css,
            families,
            totals: TotalsAll {
                font_codepoints,
                subset_count,
                font_face_count,
                subset_file_count,
                subset_bytes,
            },
            files: FilesAll { subsets },
        }
    }

    /// Serialise to pretty JSON with a trailing newline, matching
    /// `json.dumps(..., ensure_ascii=False, indent=2) + "\n"` in Python.
    pub fn to_json_string(&self) -> anyhow::Result<String> {
        let mut s = serde_json::to_string_pretty(self)?;
        s.push('\n');
        Ok(s)
    }
}

// ---------------------------------------------------------------------------
// build (single-Regular) manifest
// ---------------------------------------------------------------------------

/// One subset entry in the single-Regular `build` flow's `files.subsets[]`.
#[derive(Debug, Clone, Serialize)]
pub struct SingleSubsetEntry {
    pub name: String,
    pub path: String,
    pub nam: String,
    pub bytes: u64,
    pub codepoints: usize,
    #[serde(rename = "unicodeRange")]
    pub unicode_range: String,
    pub note: String,
}

/// `files.full` entry for the single-Regular flow.
#[derive(Debug, Clone, Serialize)]
pub struct FullFileEntry {
    pub path: String,
    pub bytes: u64,
}

/// `css` block for the single-Regular flow (just two filenames, no sizes).
#[derive(Debug, Serialize)]
pub struct CssSingle {
    pub subset: String,
    pub full: String,
}

#[derive(Debug, Serialize)]
pub struct FilesSingle {
    pub full: FullFileEntry,
    pub subsets: Vec<SingleSubsetEntry>,
}

#[derive(Debug, Serialize)]
pub struct TotalsSingle {
    #[serde(rename = "fontCodepoints")]
    pub font_codepoints: usize,
    #[serde(rename = "subsetCount")]
    pub subset_count: usize,
    #[serde(rename = "subsetBytes")]
    pub subset_bytes: u64,
    #[serde(rename = "fullBytes")]
    pub full_bytes: u64,
    #[serde(rename = "coveredCodepoints")]
    pub covered_codepoints: usize,
}

/// Manifest for the single-Regular `build` flow, the legacy shape
/// `benchmark.mjs` reads (`files.full` + `files.subsets[]`).
#[derive(Debug, Serialize)]
pub struct ManifestSingle {
    pub family: String,
    pub style: String,
    pub weight: u16,
    #[serde(rename = "fontDisplay")]
    pub font_display: String,
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    pub source: ManifestSource,
    pub css: CssSingle,
    pub files: FilesSingle,
    pub totals: TotalsSingle,
}

impl ManifestSingle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        family: impl Into<String>,
        style: impl Into<String>,
        weight: u16,
        font_display: impl Into<String>,
        source: ManifestSource,
        css: CssSingle,
        full: FullFileEntry,
        subsets: Vec<SingleSubsetEntry>,
        font_codepoints: usize,
        covered_codepoints: usize,
    ) -> Self {
        let subset_bytes: u64 = subsets.iter().map(|e| e.bytes).sum();
        let full_bytes = full.bytes;
        let subset_count = subsets.len();
        Self {
            family: family.into(),
            style: style.into(),
            weight,
            font_display: font_display.into(),
            generated_at: iso_utc_now(),
            source,
            css,
            files: FilesSingle { full, subsets },
            totals: TotalsSingle {
                font_codepoints,
                subset_count,
                subset_bytes,
                full_bytes,
                covered_codepoints,
            },
        }
    }

    /// Serialise to pretty JSON with a trailing newline.
    pub fn to_json_string(&self) -> anyhow::Result<String> {
        let mut s = serde_json::to_string_pretty(self)?;
        s.push('\n');
        Ok(s)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_size_round_trip_is_deterministic() {
        let data = b"hello world hello world hello world".repeat(10);
        let a = gzip_size(&data);
        let b = gzip_size(&data);
        assert_eq!(a, b);
        assert!(a > 0);
        assert!(a < data.len(), "compressible input should shrink");
    }

    #[test]
    fn brotli_size_compresses() {
        let data = b"a".repeat(1024);
        let s = brotli_size(&data).expect("brotli encode");
        assert!(s > 0);
        assert!(s < data.len());
    }

    #[test]
    fn iso_utc_now_has_expected_shape() {
        let s = iso_utc_now();
        // `YYYY-MM-DDTHH:MM:SS.ssssss+00:00` -> 32 chars
        assert_eq!(s.len(), 32, "got {s:?}");
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert_eq!(&s[10..11], "T");
        assert!(s.ends_with("+00:00"));
    }

    #[test]
    fn manifest_all_serialises_with_expected_keys() {
        let css_info = CssSizeInfo {
            path: "all.css".into(),
            bytes: 100,
            gzip_bytes: 50,
            brotli_bytes: Some(40),
        };
        let m = ManifestAll::new(
            "Gen Interface JP",
            "normal",
            "swap",
            ManifestSource {
                format: Some("ttf".into()),
                ttf: None,
                strategy: "google-japanese".into(),
                google_japanese_slice: Some("vendor/google.css".into()),
            },
            CssEntries {
                all: css_info.clone(),
                weights: BTreeMap::new(),
                display_weights: BTreeMap::new(),
            },
            vec![FamilyEntry {
                key: "normal".into(),
                family: "Gen Interface JP".into(),
                weights: vec![400],
            }],
            10_000,
            5,
            5,
            vec![],
        );
        let json = m.to_json_string().unwrap();
        assert!(json.contains("\"family\": \"Gen Interface JP\""));
        assert!(json.contains("\"fontDisplay\": \"swap\""));
        assert!(json.contains("\"googleJapaneseSlice\""));
        assert!(json.contains("\"subsetBytes\": 0"));
        assert!(json.ends_with('\n'));
    }
}
