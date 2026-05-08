use std::collections::BTreeMap;

/// Filename for the GitHub Release zip asset.
/// Embeds the version so a downloaded archive carries its identity in the
/// filename itself (and so a v0.1.1 site build can link the exact zip
/// matching the running font).
pub fn asset_filename(version: &str) -> String {
    format!("GenInterfaceJP-{version}.zip")
}

/// Stable URLs for the GitHub Release asset.
///
/// Only the tag-specific URL is exposed because the asset filename embeds
/// the version: `releases/latest/download/<filename>` only resolves while
/// the current "latest" release happens to ship that exact filename.
/// A versioned filename and a tag-pinned URL go together — older site
/// builds keep pointing at the asset they were built against, even after
/// a newer release becomes "latest".
pub fn github_asset_urls(repository: &str, tag: &str, version: &str) -> BTreeMap<String, String> {
    let base = format!("https://github.com/{repository}/releases/download/{tag}");
    let mut m = BTreeMap::new();
    m.insert(
        "bundle".into(),
        format!("{base}/{}", asset_filename(version)),
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_filename_embeds_version() {
        assert_eq!(asset_filename("0.1.4"), "GenInterfaceJP-0.1.4.zip");
    }

    #[test]
    fn github_asset_urls_builds_tag_pinned_bundle_url() {
        let urls = github_asset_urls("yamatoiizuka/gen-interface-jp", "v0.1.4", "0.1.4");
        let mut expected = BTreeMap::new();
        expected.insert(
            "bundle".to_string(),
            "https://github.com/yamatoiizuka/gen-interface-jp/releases/download/v0.1.4/GenInterfaceJP-0.1.4.zip".to_string(),
        );
        assert_eq!(urls, expected);
    }
}
