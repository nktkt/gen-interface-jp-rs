//! Produces unicode-range subsetted WOFF2 chunks plus CSS `@font-face` rules
//! for the Gen Interface JP webfont family.
//!
//! Two slicing strategies are supported:
//!
//! - `google-japanese` replays Google Fonts' Japanese slicing plan, mirroring
//!   the unicode-range chunks served by `fonts.googleapis.com` for CJK faces.
//! - `jis-row` uses a hand-tuned JIS row plan optimised for our own coverage
//!   and chunk-size targets.
//!
//! The generated artefacts are released both as an npm package and as a
//! GitHub Pages mirror so downstream consumers can pick whichever delivery
//! channel suits them.
//!
//! This crate is a Rust port of `source/src/webfont/build.py`.

pub mod build_runner;
pub mod cmap;
pub mod css;
pub mod families;
pub mod google_japanese;
pub mod jis;
pub mod manifest;
pub mod nam;
pub mod plan;
pub mod ranges;
pub mod subset;

pub use css::{font_face_css, font_face_css_minified, weight_css_filename};
pub use families::{WebFontFamily, WEBFONT_FAMILIES, WEIGHTS};
pub use google_japanese::{build_google_japanese_subset_plan, parse_slicing_strategy};
pub use jis::jis_row_codepoints;
pub use plan::{build_subset_plan, WebFontSubset};
pub use ranges::{codepoints_from_ranges, format_unicode_range, merge_codepoints_to_ranges};
pub use subset::{build_full_woff2, build_woff2_subset};

pub const FAMILY_NAME: &str = "Gen Interface JP";
pub const WEIGHT_DEFAULT: u16 = 400;
pub const STYLE: &str = "normal";
pub const DISPLAY: &str = "swap";
