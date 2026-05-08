//! Version resolution for release builds.
//!
//! Ports `project_version`, `normalized_version`, and `release_tag` from
//! `../source/src/release/build.py` (lines ~48-63). The Python implementation
//! reads from `pyproject.toml`; this Rust port reads from the workspace
//! `Cargo.toml` instead.

use std::path::Path;

use anyhow::{anyhow, Context};
use regex::Regex;

/// Read the workspace `Cargo.toml`'s `[workspace.package].version` value.
///
/// Looks at `<workspace_root>/Cargo.toml` and returns the first
/// `version = "..."` line found. The typical layout has the
/// `[workspace.package]` table near the top of the file, so the first match
/// is the workspace version.
pub fn project_version(workspace_root: &Path) -> anyhow::Result<String> {
    let cargo_toml = workspace_root.join("Cargo.toml");
    let contents = std::fs::read_to_string(&cargo_toml)
        .with_context(|| format!("failed to read {}", cargo_toml.display()))?;

    // Mirrors the Python: `re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)`.
    let re = Regex::new(r#"(?m)^version\s*=\s*"([^"]+)""#).expect("static regex compiles");

    let captures = re.captures(&contents).ok_or_else(|| {
        anyhow!(
            "could not read project version from {}",
            cargo_toml.display()
        )
    })?;

    Ok(captures
        .get(1)
        .expect("regex has one capture group")
        .as_str()
        .to_owned())
}

/// Resolve the canonical version string. Priority:
///   1. explicit `version` arg (if `Some`)
///   2. `GITHUB_REF_NAME` env var (CI tag-driven publish)
///   3. [`project_version`] from `Cargo.toml`
///
/// The leading `"v"` is stripped if present, so the returned string is
/// always in plain numeric form (e.g. `"0.1.4"`).
pub fn normalized_version(version: Option<&str>, workspace_root: &Path) -> anyhow::Result<String> {
    let raw = match version {
        Some(v) => v.to_owned(),
        None => match std::env::var("GITHUB_REF_NAME") {
            Ok(v) => v,
            Err(_) => project_version(workspace_root)?,
        },
    };

    Ok(strip_leading_v(&raw).to_owned())
}

/// Convert a version string into a tag form (`v` prefix).
///
/// `release_tag("0.1.4")` -> `"v0.1.4"`. Already-prefixed inputs are returned
/// unchanged: `release_tag("v0.1.4")` -> `"v0.1.4"`.
pub fn release_tag(version: &str) -> String {
    if version.starts_with('v') {
        version.to_owned()
    } else {
        format!("v{version}")
    }
}

fn strip_leading_v(s: &str) -> &str {
    s.strip_prefix('v').unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_tag_adds_prefix_when_missing() {
        assert_eq!(release_tag("0.1.4"), "v0.1.4");
    }

    #[test]
    fn release_tag_is_idempotent() {
        assert_eq!(release_tag("v0.1.4"), "v0.1.4");
    }

    #[test]
    fn release_tag_handles_short_version() {
        assert_eq!(release_tag("v1.2"), "v1.2");
    }

    #[test]
    fn project_version_reads_first_version_line() {
        let dir = tempdir();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = \"1.2.3\"\n",
        )
        .expect("write tempfile");

        let v = project_version(dir.path()).expect("read version");
        assert_eq!(v, "1.2.3");
    }

    #[test]
    fn project_version_picks_workspace_package_version() {
        // Realistic-shaped workspace Cargo.toml. The regex's first match
        // lands inside `[workspace.package]` because that's where the first
        // bare `version = "..."` line appears.
        let dir = tempdir();
        let toml = r#"
[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.4.2"
edition = "2021"
"#;
        std::fs::write(dir.path().join("Cargo.toml"), toml).expect("write tempfile");

        let v = project_version(dir.path()).expect("read version");
        assert_eq!(v, "0.4.2");
    }

    #[test]
    fn project_version_errors_when_missing() {
        let dir = tempdir();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n")
            .expect("write tempfile");

        let err = project_version(dir.path()).expect_err("should fail");
        assert!(err.to_string().contains("could not read project version"));
    }

    #[test]
    fn normalized_version_strips_leading_v_from_arg() {
        let dir = tempdir();
        let v = normalized_version(Some("v0.1.4"), dir.path()).expect("normalize");
        assert_eq!(v, "0.1.4");
    }

    #[test]
    fn normalized_version_passes_plain_arg_through() {
        let dir = tempdir();
        let v = normalized_version(Some("0.1.4"), dir.path()).expect("normalize");
        assert_eq!(v, "0.1.4");
    }

    /// Tiny inline tempdir helper so we don't pull in `tempfile` just for
    /// these tests. Cleaned up on `Drop`.
    fn tempdir() -> TempDir {
        let mut path = std::env::temp_dir();
        let unique = format!(
            "gen-release-version-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        );
        path.push(unique);
        std::fs::create_dir_all(&path).expect("create tempdir");
        TempDir { path }
    }

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
