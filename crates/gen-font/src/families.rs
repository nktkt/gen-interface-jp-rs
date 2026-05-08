//! Family / weight matrix and shared merge constants for Gen Interface JP.
//!
//! Mirrors the `FAMILIES` dict, `SUB_EXCLUDE_CODEPOINTS` list, and the
//! `BASELINE_OFFSET` / `SCALE` constants from `src/font/build.py`.
//!
//! Families:
//!   - Gen Interface JP         : Inter        + proportional Noto, tracking +30 (kana +40)
//!   - Gen Interface JP Display : InterDisplay + proportional Noto, tracking 0

/// Configuration for a single output family (`normal` or `display`).
///
/// `glyph_spacing` holds per-glyph sidebearing tweaks applied after tracking:
/// each entry maps a character to a `(lsb_delta, rsb_delta)` pair in design
/// units. Positive deltas add whitespace, negative tighten. Populate when a
/// specific glyph needs a manual margin nudge that palt + tracking alone
/// can't reach.
#[derive(Debug, Clone)]
pub struct FamilyConfig {
    /// Lookup key: `"normal"` or `"display"`.
    pub key: &'static str,
    /// Output family name (e.g. `"Gen Interface JP"` / `"... Display"`).
    pub family_name: &'static str,
    /// Inter master prefix: `"Inter"` or `"InterDisplay"`.
    pub inter_prefix: &'static str,
    /// Tracking applied to Latin glyphs, in design units (LSB +half, RSB +half).
    pub tracking: i32,
    /// Optional kana-specific tracking override.
    pub tracking_kana: Option<i32>,
    /// Whether to halve palt-derived punctuation sidebearings.
    pub half_palt_punct: bool,
    /// Filesystem prefix for intermediate / dist artifacts.
    pub folder_prefix: &'static str,
    /// Per-glyph sidebearing overrides applied after tracking.
    pub glyph_spacing: &'static [(char, (i32, i32))],
    /// Horizontal scale applied to Noto outlines (default 1.0).
    pub x_scale: f32,
}

impl FamilyConfig {
    /// Returns the `FamilyConfig` whose `key` matches, or `None`.
    pub fn lookup(key: &str) -> Option<&'static FamilyConfig> {
        FAMILIES.iter().find(|f| f.key == key)
    }
}

/// All output families. Mirrors the Python `FAMILIES` dict.
pub const FAMILIES: &[FamilyConfig] = &[
    FamilyConfig {
        key: "normal",
        family_name: "Gen Interface JP",
        inter_prefix: "Inter",
        tracking: 30,
        tracking_kana: Some(40),
        half_palt_punct: true,
        folder_prefix: "GenInterfaceJP",
        glyph_spacing: &[('く', (30, 0))],
        x_scale: 1.0,
    },
    FamilyConfig {
        key: "display",
        family_name: "Gen Interface JP Display",
        inter_prefix: "InterDisplay",
        tracking: 0,
        tracking_kana: Some(0),
        half_palt_punct: true,
        folder_prefix: "GenInterfaceJPDisplay",
        glyph_spacing: &[('く', (30, 0))],
        x_scale: 1.0,
    },
];

/// Codepoints whose glyphs should stay sourced from the base Noto font even
/// when Inter/InterDisplay also encodes them. Forwarded as
/// `subFont.excludeCodepoints` to font-baker, which strips them from the sub
/// cmap before merge so the base outline survives. Edit to tune merge policy.
///
/// These are CJK-conventional symbols (circled digits, circled letters,
/// kome-jirushi, circled math operators, large circle): Japanese readers
/// expect Noto's rendering of these glyphs, not Inter's Latin-styled
/// versions, even when Inter encodes them.
///
/// `◎` (U+25CE) is intentionally absent: Inter does not encode U+25CE itself,
/// but encodes U+0298 with glyph name `uni25CE` which used to silently
/// overwrite Noto's bullseye. font-baker now detects that glyph-name
/// collision and renames the sub glyph to `uni25CE.sub`, so excluding the
/// codepoint here is unnecessary.
///
/// Note: Dingbat Sans-Serif Circled has no 0 (Unicode never assigned one).
/// `➉` (U+2789) exists but Inter does not encode it, so excluding it is moot.
/// Negative-circled families (`❶`-`❿` U+2776-U+277F, `➊`-`➓` U+278A-U+2793)
/// are absent from Inter entirely, so they fall through to Noto without help.
pub const SUB_EXCLUDE_CODEPOINTS: &[&str] = &[
    "U+2460-U+2469",   // ① ② ③ ④ ⑤ ⑥ ⑦ ⑧ ⑨
    "U+24EA",          // ⓪
    "U+2780-U+2788",   // ➀-➈ (Dingbat Sans-Serif Circled aliases of ①-⑨)
    "U+24B6-U+24CF",   // Ⓐ-Ⓩ
    "U+1F130-U+1F149", // 🄰-🅉
    "U+203B",          // ※
    "U+2295",          // ⊕
    "U+2296",          // ⊖
    "U+2297",          // ⊗
    "U+2298",          // ⊘
    "U+25EF",          // ◯
];

/// Vertical alignment between Inter and Noto.
///
/// When the merged font hands its baseline to Inter (`metricsSource: "sub"`),
/// Noto sits a touch low — its CJK ideographs visually rest below the Latin
/// x-height baseline. `BASELINE_OFFSET` nudges every Noto glyph up by 25
/// units so capitals and ideographs share an optical baseline.
pub const BASELINE_OFFSET: i32 = 25;

/// Horizontal scale applied to Noto outlines.
///
/// Shrinks Noto to ~92.5% so a CJK character lines up in width with the
/// cap-height of Inter at the same nominal point size — a typographic
/// convention for Latin/CJK pairing where CJK is slightly down-scaled to
/// feel proportionate.
pub const SCALE: f32 = 0.925;
