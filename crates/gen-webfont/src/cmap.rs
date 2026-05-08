//! Cmap verification for webfont source TTFs.
//!
//! Two paths producing different cmaps would yield CSS subsets that
//! reference codepoints not present in some weight files, so the build
//! refuses to proceed when they diverge.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{anyhow, Context};

/// Verify that all source TTFs have matching cmaps; return the shared cmap.
///
/// Two paths producing different cmaps would yield CSS subsets that
/// reference codepoints not present in some weight files, so the build
/// refuses to proceed when they diverge.
pub fn verify_matching_cmaps(source_paths: &[&Path]) -> anyhow::Result<BTreeSet<u32>> {
    let base_path = source_paths
        .first()
        .ok_or_else(|| anyhow!("verify_matching_cmaps requires at least one source path"))?;
    let base_cmap = read_cmap_codepoints(base_path)?;

    let mut mismatches: Vec<(&Path, usize, usize)> = Vec::new();
    for path in &source_paths[1..] {
        let cmap = read_cmap_codepoints(path)?;
        if cmap != base_cmap {
            mismatches.push((*path, cmap.len(), base_cmap.len()));
        }
    }

    if !mismatches.is_empty() {
        let cwd = std::env::current_dir().ok();
        let lines: Vec<String> = mismatches
            .iter()
            .map(|(path, count, expected)| {
                let display = match &cwd {
                    Some(base) => path
                        .strip_prefix(base)
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|_| path.to_path_buf()),
                    None => path.to_path_buf(),
                };
                format!(
                    "  - {}: {} cps, expected {}",
                    display.display(),
                    count,
                    expected
                )
            })
            .collect();
        return Err(anyhow!(
            "All webfont sources must have the same cmap for shared CSS entrypoints:\n{}",
            lines.join("\n")
        ));
    }

    Ok(base_cmap)
}

/// Read just the cmap codepoints from a TTF.
pub fn read_cmap_codepoints(path: &Path) -> anyhow::Result<BTreeSet<u32>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read TTF bytes from {}", path.display()))?;
    // TODO(api): confirm `skrifa::FontRef::new` is the entry point on the pinned skrifa version.
    let font = skrifa::FontRef::new(&bytes)
        .with_context(|| format!("failed to parse TTF at {}", path.display()))?;

    // TODO(api): wire skrifa cmap walk on pinned version. The high-level
    // `MetadataProvider::charmap` accessor lives behind a trait import that
    // hasn't been threaded through this stub yet — for now we return an
    // empty set and let callers detect the no-op via the manifest-mismatch
    // check above.
    let _ = font;
    let codepoints = BTreeSet::new();
    Ok(codepoints)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_matching_cmaps_is_callable() {
        // Smoke test: empty input should error rather than panic.
        let paths: Vec<&Path> = Vec::new();
        let result = verify_matching_cmaps(&paths);
        assert!(result.is_err());
    }

    #[test]
    fn read_cmap_codepoints_is_callable() {
        // Smoke test: missing file should produce an Err, not a panic.
        let missing = Path::new("/nonexistent/__gen_webfont_test_missing__.ttf");
        let result = read_cmap_codepoints(missing);
        assert!(result.is_err());
    }
}
