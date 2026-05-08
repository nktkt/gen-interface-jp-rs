//! Rust port of the Python font build pipeline.
//!
//! This crate bakes Noto Sans CJK Variable fonts at fixed weights,
//! proportionalises their CJK glyphs, and merges the result with Inter
//! to produce the final font families. See `docs/ARCHITECTURE.md` for
//! full pipeline details.

pub mod baker;
pub mod build;
pub mod classify;
pub mod families;
pub mod glyph;
pub mod glyph_spacing;
pub mod palt;
pub mod proportional;
pub mod site_subset;
pub mod strip_extreme;
pub mod tracking;
pub mod weights;
pub mod x_scale;

pub use build::{build_one, BuildResult};
pub use families::{FamilyConfig, BASELINE_OFFSET, FAMILIES, SCALE, SUB_EXCLUDE_CODEPOINTS};
pub use weights::{WeightSpec, WEIGHTS};
