//! Release artifact generation for `GenInterfaceJP`.
//!
//! - Outputs `dist/release/github/GenInterfaceJP-<version>.zip` (TTF, both families × all weights)
//! - Outputs `dist/release/npm/` (subset webfont package for npm publishing)
//! - Outputs `dist/release/webfonts/gen-interface-jp/` (Pages-hosted mirror)

pub mod build;
pub mod github;
pub mod npm;
pub mod release_zip;
pub mod version;

pub use build::{build_release, BuildReleaseArgs};
pub use github::{asset_filename, github_asset_urls};
pub use release_zip::{family_files, ofl_text, write_zip, ReleaseFile};
pub use version::{normalized_version, project_version, release_tag};

pub const DEFAULT_REPOSITORY: &str = "yamatoiizuka/gen-interface-jp";
