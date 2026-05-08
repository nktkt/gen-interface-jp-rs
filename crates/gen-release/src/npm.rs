//! NPM package staging for the webfont release.
//!
//! Ports `require_webfont_package`, `copy_webfont_package`,
//! `write_npm_license_files`, and `write_npm_package` from
//! `source/src/release/build.py`.

use std::path::Path;

use anyhow::{anyhow, Context};
use walkdir::WalkDir;

/// Verify the webfont source directory has the required outputs.
pub fn require_webfont_package(source: &Path) -> anyhow::Result<()> {
    let required = [
        source.join("all.css"),
        source.join("manifest.json"),
        source.join("nam"),
        source.join("w"),
    ];
    let missing: Vec<String> = required
        .iter()
        .filter(|p| !p.exists())
        .map(|p| p.display().to_string())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let lines = missing
        .iter()
        .map(|p| format!("  - {p}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(anyhow!(
        "Missing webfont build outputs:\n{lines}\nRun: just webfont"
    ))
}

/// Copy `source/` -> `out_dir/`, removing `out_dir/nam` if `include_nam` is false.
/// Replaces `out_dir` if it already exists. Prints `Wrote {out_dir}`.
pub fn copy_webfont_package(
    source: &Path,
    out_dir: &Path,
    include_nam: bool,
) -> anyhow::Result<()> {
    require_webfont_package(source)?;
    if out_dir.exists() {
        std::fs::remove_dir_all(out_dir)
            .with_context(|| format!("removing existing {}", out_dir.display()))?;
    }
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    for entry in WalkDir::new(source) {
        let entry = entry.with_context(|| format!("walking {}", source.display()))?;
        let rel = entry
            .path()
            .strip_prefix(source)
            .expect("walked entry must be under source");
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest = out_dir.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)
                .with_context(|| format!("creating {}", dest.display()))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            std::fs::copy(entry.path(), &dest).with_context(|| {
                format!("copying {} -> {}", entry.path().display(), dest.display())
            })?;
        }
    }

    if !include_nam {
        let _ = std::fs::remove_dir_all(out_dir.join("nam"));
    }
    println!("Wrote {}", out_dir.display());
    Ok(())
}

/// Write `out_dir/OFL.txt` from the composed OFL body.
pub fn write_npm_license_files(out_dir: &Path, ofl_body: &str) -> anyhow::Result<()> {
    let path = out_dir.join("OFL.txt");
    std::fs::write(&path, ofl_body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Write `out_dir/package.json` with name="gen-interface-jp", license="OFL-1.1", etc.
pub fn write_npm_package(out_dir: &Path, version: &str, repository: &str) -> anyhow::Result<()> {
    let pkg = serde_json::json!({
        "name": "gen-interface-jp",
        "version": version,
        "description": "Gen Interface JP web font subsets",
        "style": "all.css",
        "files": ["*.css", "manifest.json", "OFL.txt", "w"],
        "repository": {
            "type": "git",
            "url": format!("git+https://github.com/{repository}.git"),
        },
        "homepage": format!("https://github.com/{repository}"),
        "license": "OFL-1.1",
    });
    let body = serde_json::to_string_pretty(&pkg)? + "\n";
    let path = out_dir.join("package.json");
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Minimal scoped tempdir helper: avoids pulling in `tempfile` if it's not
    /// already a workspace dep. Removes the directory on drop.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            path.push(format!(
                "gen-release-npm-{label}-{nanos}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create tempdir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn write_npm_package_writes_valid_json() {
        let tmp = TempDir::new("pkg");
        write_npm_package(tmp.path(), "1.2.3", "owner/repo").expect("write_npm_package");

        let body =
            std::fs::read_to_string(tmp.path().join("package.json")).expect("read package.json");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");

        assert_eq!(parsed["name"], "gen-interface-jp");
        assert_eq!(parsed["version"], "1.2.3");
        assert_eq!(parsed["license"], "OFL-1.1");
        assert_eq!(parsed["style"], "all.css");
        assert_eq!(
            parsed["repository"]["url"],
            "git+https://github.com/owner/repo.git"
        );
        assert_eq!(parsed["homepage"], "https://github.com/owner/repo");
        assert!(body.ends_with('\n'), "trailing newline preserved");
    }

    #[test]
    fn require_webfont_package_errors_when_dir_missing() {
        let tmp = TempDir::new("missing");
        let nonexistent = tmp.path().join("does-not-exist");
        let err =
            require_webfont_package(&nonexistent).expect_err("should error for missing source");
        let msg = format!("{err}");
        assert!(
            msg.starts_with("Missing webfont build outputs:"),
            "unexpected error message: {msg}"
        );
        assert!(msg.contains("Run: just webfont"), "missing run hint: {msg}");
    }
}
