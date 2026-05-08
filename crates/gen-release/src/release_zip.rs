use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use zip::write::{FileOptions, ZipWriter};
use zip::CompressionMethod;

use gen_font::{FAMILIES, WEIGHTS};

#[derive(Debug, Clone)]
pub struct ReleaseFile {
    pub path: PathBuf,
    pub archive_name: String,
}

/// Collect every TTF expected at dist/ttf/ for the GitHub Release zip.
///
/// Archive paths are nested under a version-stamped root folder
/// (`GenInterfaceJP-<version>/`) so unzipping cleanly drops the files
/// into a single labelled directory rather than spilling family folders
/// into whatever the user's working directory is.
pub fn family_files(dist_ttf_root: &Path, version: &str) -> Vec<ReleaseFile> {
    let mut files = Vec::new();
    for family in FAMILIES {
        for weight in WEIGHTS {
            let filename = format!("{}-{}.ttf", family.folder_prefix, weight.weight_name);
            let path = dist_ttf_root.join(&family.family_name).join(&filename);
            let archive_name = format!(
                "GenInterfaceJP-{version}/{}/{}",
                family.family_name, filename
            );
            files.push(ReleaseFile { path, archive_name });
        }
    }
    files
}

/// Bail if any ReleaseFile.path is missing on disk; print a multi-line list.
pub fn require_files(files: &[ReleaseFile]) -> Result<()> {
    let missing: Vec<String> = files
        .iter()
        .filter(|f| !f.path.is_file())
        .map(|f| f.path.display().to_string())
        .collect();
    if !missing.is_empty() {
        let lines = missing
            .iter()
            .map(|p| format!("  - {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(anyhow!(
            "Missing release input files:\n{lines}\nRun: make font"
        ));
    }
    Ok(())
}

/// Write a zip containing on-disk files plus optional inline text entries.
///
/// `inline` maps archive path → string content; used to add the OFL text
/// directly without staging it on disk first.
pub fn write_zip(
    path: &Path,
    files: &[ReleaseFile],
    inline: &BTreeMap<String, String>,
) -> Result<()> {
    require_files(files)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }

    let file = fs::File::create(path)
        .with_context(|| format!("creating zip file {}", path.display()))?;
    let mut zip = ZipWriter::new(file);
    let opts: FileOptions<()> = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(9));

    for entry in files {
        let bytes = fs::read(&entry.path)
            .with_context(|| format!("reading {}", entry.path.display()))?;
        zip.start_file(&entry.archive_name, opts)
            .with_context(|| format!("starting zip entry {}", entry.archive_name))?;
        zip.write_all(&bytes)
            .with_context(|| format!("writing zip entry {}", entry.archive_name))?;
    }

    for (arc_name, content) in inline {
        zip.start_file(arc_name, opts)
            .with_context(|| format!("starting zip entry {arc_name}"))?;
        zip.write_all(content.as_bytes())
            .with_context(|| format!("writing zip entry {arc_name}"))?;
    }

    zip.finish().context("finalising zip archive")?;
    println!("Wrote {}", path.display());
    Ok(())
}

/// Compose the OFL.txt body shipped alongside the TTFs.
/// Reads `<vendor_inter_ofl_path>` and prepends our copyright lines.
pub fn ofl_text(inter_ofl_path: &Path) -> Result<String> {
    let inter_license = fs::read_to_string(inter_ofl_path)
        .with_context(|| format!("reading {}", inter_ofl_path.display()))?;
    let (_, ofl_body) = inter_license
        .split_once("\n\n")
        .ok_or_else(|| anyhow!("expected blank line in {}", inter_ofl_path.display()))?;
    let copyright_lines = [
        "Copyright 2026 The Gen Interface JP Project Authors (https://github.com/yamatoiizuka/gen-interface-jp)",
        "Copyright (c) 2016 The Inter Project Authors (https://github.com/rsms/inter)",
        "Copyright 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'",
    ];
    Ok(format!("{}\n\n{}", copyright_lines.join("\n"), ofl_body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ofl_text_prepends_copyright_lines() {
        let path = std::env::temp_dir().join(format!(
            "gen_release_ofl_test_{}.txt",
            std::process::id()
        ));
        std::fs::write(&path, b"Copyright stuff\n\nOFL body line").unwrap();
        let out = ofl_text(&path).expect("ofl_text");
        let _ = std::fs::remove_file(&path);
        let expected = "Copyright 2026 The Gen Interface JP Project Authors (https://github.com/yamatoiizuka/gen-interface-jp)\n\
Copyright (c) 2016 The Inter Project Authors (https://github.com/rsms/inter)\n\
Copyright 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'\n\
\n\
OFL body line";
        assert_eq!(out, expected);
    }
}
