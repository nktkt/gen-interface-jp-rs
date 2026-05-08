use std::collections::BTreeSet;
use std::path::Path;

/// Write a `.nam` codepoint listing in googlefonts/nam-files format.
///
/// One codepoint per line as `0x{:04X}`, preceded by two header comment
/// lines. Codepoints are sorted ascending and deduplicated.
///
/// Creates parent directories if needed.
pub fn write_nam(out_path: &Path, codepoints: &[u32], note: &str) -> anyhow::Result<()> {
    let sorted: BTreeSet<u32> = codepoints.iter().copied().collect();

    let mut content = String::new();
    content.push_str(&format!("# {}\n", note));
    content.push_str("# One codepoint per line, nam-files style.\n");
    for cp in &sorted {
        content.push_str(&format!("0x{:04X}\n", cp));
    }

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unique_temp_path(name: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("gen_webfont_nam_test_{}_{}_{}", pid, nanos, name))
    }

    #[test]
    fn writes_basic_layout() {
        let path = unique_temp_path("basic.nam");
        let _ = fs::remove_file(&path);

        write_nam(&path, &[0x41, 0x3042], "test note").expect("write_nam failed");

        let content = fs::read_to_string(&path).expect("read failed");
        let expected = "# test note\n\
                        # One codepoint per line, nam-files style.\n\
                        0x0041\n\
                        0x3042\n";
        assert_eq!(content, expected);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn dedups_and_sorts() {
        let path = unique_temp_path("dedup.nam");
        let _ = fs::remove_file(&path);

        write_nam(&path, &[0x42, 0x41, 0x42], "dedup").expect("write_nam failed");

        let content = fs::read_to_string(&path).expect("read failed");
        let expected = "# dedup\n\
                        # One codepoint per line, nam-files style.\n\
                        0x0041\n\
                        0x0042\n";
        assert_eq!(content, expected);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn auto_extends_for_large_codepoints() {
        let path = unique_temp_path("large.nam");
        let _ = fs::remove_file(&path);

        // 0x1F600 (emoji) is > 0xFFFF, should produce 5 hex digits.
        write_nam(&path, &[0x41, 0x1F600], "large").expect("write_nam failed");

        let content = fs::read_to_string(&path).expect("read failed");
        let expected = "# large\n\
                        # One codepoint per line, nam-files style.\n\
                        0x0041\n\
                        0x1F600\n";
        assert_eq!(content, expected);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn creates_parent_directories() {
        let base = unique_temp_path("nested_dir");
        let nested = base.join("sub").join("file.nam");
        let _ = fs::remove_dir_all(&base);

        write_nam(&nested, &[0x41], "nested").expect("write_nam failed");

        let content = fs::read_to_string(&nested).expect("read failed");
        assert!(content.contains("0x0041\n"));
        assert!(content.ends_with("\n"));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn empty_codepoints_writes_only_header() {
        let path = unique_temp_path("empty.nam");
        let _ = fs::remove_file(&path);

        write_nam(&path, &[], "empty").expect("write_nam failed");

        let content = fs::read_to_string(&path).expect("read failed");
        let expected = "# empty\n# One codepoint per line, nam-files style.\n";
        assert_eq!(content, expected);

        let _ = fs::remove_file(&path);
    }
}
