//! Top-level release assembly.
//!
//! Ports `build_release` and the path/repository defaults from
//! `../source/src/release/build.py` (lines ~199-260). Stitches together the
//! `version`, `release_zip`, `npm`, and `github` modules into a single
//! end-to-end build that produces:
//!
//! - `dist/release/github/GenInterfaceJP-<version>.zip` (TTFs + OFL.txt)
//! - `dist/release/npm/` (subset webfont package, no `nam/`)
//! - `dist/release/webfonts/gen-interface-jp/` (Pages-hosted mirror, with `nam/`)
//! - `dist/release/manifest.json` (release index for downstream consumers)

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context;

use crate::github::*;
use crate::npm::*;
use crate::release_zip::*;
use crate::version::*;

/// Inputs for [`build_release`]. Mirrors the `argparse.Namespace` the Python
/// CLI passes around, but with explicit roots so the function does not depend
/// on the process CWD.
#[derive(Debug, Clone)]
pub struct BuildReleaseArgs {
    /// Workspace root used for `project_version()` lookup (reads `Cargo.toml`).
    pub workspace_root: PathBuf,
    /// `../source/` root — vendor fonts and `dist/ttf/` live here.
    pub source_root: PathBuf,
    /// Optional explicit version override. Falls through to `GITHUB_REF_NAME`
    /// then to the workspace `Cargo.toml` version.
    pub version: Option<String>,
    /// GitHub `owner/repo`, e.g. `"yamatoiizuka/gen-interface-jp"`.
    pub repository: String,
    /// Release output directory (typically `dist/release/`).
    pub output: PathBuf,
    /// Built webfont source directory (typically
    /// `dist/webfont/gen-interface-jp/`).
    pub webfont_source: PathBuf,
}

/// Release manifest written to `dist/release/manifest.json`. Field renames
/// match the Python output exactly so downstream consumers (the site build,
/// CI publish steps) keep parsing it without changes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReleaseManifest {
    pub version: String,
    pub tag: String,
    #[serde(rename = "githubRepository")]
    pub github_repository: String,
    #[serde(rename = "githubReleaseAssets")]
    pub github_release_assets: BTreeMap<String, String>,
    pub webfonts: BTreeMap<String, String>,
}

/// Assemble the full release: GitHub zip, npm package, Pages mirror, and
/// `manifest.json`. Returns the manifest after writing it to disk.
pub fn build_release(args: &BuildReleaseArgs) -> anyhow::Result<ReleaseManifest> {
    let version = normalized_version(args.version.as_deref(), &args.workspace_root)?;
    let tag = release_tag(&version);

    // Ensure `args.output` exists before canonicalising — `canonicalize`
    // requires the path to resolve on disk.
    std::fs::create_dir_all(&args.output)
        .with_context(|| format!("creating {}", args.output.display()))?;
    let release_dir = args.output.canonicalize().with_context(|| {
        format!("canonicalising release output {}", args.output.display())
    })?;
    let github_dir = release_dir.join("github");
    let npm_dir = release_dir.join("npm");
    let webfont_out = release_dir.join("webfonts").join("gen-interface-jp");

    // GitHub Release ships TTFs only. Web delivery (subset WOFF2 chunks
    // behind unicode-range) flows through the npm package below; full
    // WOFF2 single-file is intentionally not redistributed.
    //
    // OFL.txt is added inline alongside the TTFs at the version-rooted
    // archive directory so anyone unzipping the bundle has the license
    // immediately at hand (matches OFL §2's "include this license"
    // requirement for redistribution).
    let dist_ttf_root = args.source_root.join("dist/ttf");
    let inter_ofl_path = args
        .source_root
        .join("vendor/fonts/Inter-4.1/LICENSE.txt");
    let ofl_body = ofl_text(&inter_ofl_path)?;

    let archive_root = format!("GenInterfaceJP-{version}");
    let mut inline = BTreeMap::new();
    inline.insert(format!("{archive_root}/OFL.txt"), ofl_body.clone());

    let zip_path = github_dir.join(asset_filename(&version));
    write_zip(&zip_path, &family_files(&dist_ttf_root, &version), &inline)?;

    // Canonicalise the webfont source so symlinked dist/ paths resolve to
    // their real location before the copy pass walks the tree.
    let source = args.webfont_source.canonicalize().with_context(|| {
        format!(
            "canonicalising webfont source {}",
            args.webfont_source.display()
        )
    })?;
    copy_webfont_package(&source, &npm_dir, /* include_nam */ false)?;
    write_npm_license_files(&npm_dir, &ofl_body)?;
    write_npm_package(&npm_dir, &version, &args.repository)?;
    copy_webfont_package(&source, &webfont_out, /* include_nam */ true)?;

    let mut webfonts = BTreeMap::new();
    webfonts.insert("npmPackage".into(), "npm".into());
    webfonts.insert("npmAllCss".into(), "npm/all.css".into());
    webfonts.insert(
        "staticAllCss".into(),
        "webfonts/gen-interface-jp/all.css".into(),
    );

    let manifest = ReleaseManifest {
        version: version.clone(),
        tag: tag.clone(),
        github_repository: args.repository.clone(),
        github_release_assets: github_asset_urls(&args.repository, &tag, &version),
        webfonts,
    };

    let manifest_path = release_dir.join("manifest.json");
    let body = serde_json::to_string_pretty(&manifest)
        .context("serialising release manifest")?
        + "\n";
    std::fs::write(&manifest_path, body)
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    println!("Wrote {}", manifest_path.display());

    Ok(manifest)
}
