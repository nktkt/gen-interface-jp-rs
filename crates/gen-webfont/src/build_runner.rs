//! High-level build pipelines.
//!
//! Ported from `source/src/webfont/build.py` (`build_all`, `build`,
//! and the small helpers they call). Two flows live here:
//!
//! - [`build_all`] — multi-weight, multi-family pipeline producing
//!   `dist/webfont/gen-interface-jp/`. This is what the public release
//!   bundle is built from.
//! - [`build_single`] — single-Regular benchmark pipeline. Produces
//!   subset chunks plus a single full-WOFF2 baseline so `benchmark.mjs`
//!   can compare per-page subset delivery against monolithic delivery.
//!
//! The Python reference uses a `concurrent.futures.ProcessPoolExecutor`
//! to parallelise the per-subset WOFF2 work. Here we use `rayon` for the
//! same fan-out — by routing results back through stable indices we keep
//! the manifest reproducible regardless of completion order.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use rayon::prelude::*;

use crate::cmap;
use crate::css::{font_face_css, font_face_css_minified, weight_css_filename};
use crate::families::{WEBFONT_FAMILIES, WEIGHTS};
use crate::google_japanese::build_google_japanese_subset_plan;
use crate::manifest::{
    css_size_info, CssEntries, CssSingle, FamilyEntry, FullFileEntry, ManifestAll, ManifestSingle,
    ManifestSource, SingleSubsetEntry, SubsetFileEntry,
};
use crate::nam::write_nam;
use crate::plan::{build_subset_plan, WebFontSubset};
use crate::ranges::format_unicode_range;
use crate::subset::{build_full_woff2, build_woff2_subset};
use crate::{DISPLAY, FAMILY_NAME, STYLE, WEIGHT_DEFAULT};

// ---------------------------------------------------------------------------
// Public API: argument structs + strategy enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum SubsetStrategy {
    /// Hand-tuned JIS-row plan (Python `--strategy=jis-row`).
    JisRow,
    /// Google Fonts' Japanese slicing strategy (Python `--strategy=google-japanese`).
    GoogleJapanese,
}

impl SubsetStrategy {
    /// Wire string used in the manifest `source.strategy` field — must match
    /// what the Python writes so downstream tooling stays compatible.
    pub fn as_manifest_str(self) -> &'static str {
        match self {
            SubsetStrategy::JisRow => "jis-row",
            SubsetStrategy::GoogleJapanese => "google-japanese",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuildAllArgs {
    /// Workspace root, used to relativise paths in the manifest.
    pub root: PathBuf,
    /// Output directory (e.g. `dist/webfont/gen-interface-jp/`).
    pub output: PathBuf,
    /// `rm -rf output` before building when `true`.
    pub clean: bool,
    pub strategy: SubsetStrategy,
    /// Parallel WOFF2 worker count. Clamped to `>= 1`.
    pub jobs: usize,
    /// Number of `jp-kanji-extra-NN` slices for the JIS-row strategy.
    pub extra_han_slices: usize,
    /// Skip the "remaining codepoints" tail for the Google-Japanese strategy.
    pub no_remaining: bool,
    /// Number of slices for the Google-Japanese remaining tail.
    pub remaining_slices: usize,
    /// Path to `googlefonts/nam-files/slices/japanese_default.txt`.
    pub google_japanese_slice: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BuildSingleArgs {
    pub root: PathBuf,
    /// Source TTF (Regular weight only — this is the benchmark pipeline).
    pub ttf: PathBuf,
    pub output: PathBuf,
    pub clean: bool,
    pub strategy: SubsetStrategy,
    pub extra_han_slices: usize,
    pub no_remaining: bool,
    pub remaining_slices: usize,
    pub google_japanese_slice: PathBuf,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// One `@font-face` rule's source data, kept around for CSS generation.
struct FaceEntry {
    family: String,
    family_key: String,
    weight: u16,
    path: String,
    unicode_range: String,
}

/// Best-effort canonicalisation: if the path already exists, canonicalise it;
/// otherwise just absolutise via `cwd + path` so we still get an absolute
/// path the rest of the pipeline can use without surprises.
fn absolutize(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        path.canonicalize()
    } else if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir()?;
        Ok(cwd.join(path))
    }
}

/// Mirror Python's `_relative_to_root`: if `path` is inside `root`, return the
/// relative form; otherwise stringify the absolute path. Always uses forward
/// slashes (we control the manifest so cross-platform consumers see the same
/// shape regardless of build host).
fn relative_to_root(path: &Path, root: &Path) -> String {
    let abs = absolutize(path).unwrap_or_else(|_| path.to_path_buf());
    let root_abs = absolutize(root).unwrap_or_else(|_| root.to_path_buf());
    match abs.strip_prefix(&root_abs) {
        Ok(rel) => path_to_forward_slashes(rel),
        Err(_) => path_to_forward_slashes(&abs),
    }
}

/// Format a `Path` with forward slashes regardless of host OS — manifests
/// are consumed by JS/CSS tooling that uses POSIX paths.
fn path_to_forward_slashes(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::Normal(os) => parts.push(os.to_string_lossy().into_owned()),
            std::path::Component::RootDir => parts.push(String::new()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => parts.push("..".to_string()),
            std::path::Component::Prefix(p) => {
                parts.push(p.as_os_str().to_string_lossy().into_owned())
            }
        }
    }
    parts.join("/")
}

/// Resolve the source font path for a given (family, weight) pair.
fn source_font_path(
    root: &Path,
    dist_folder: &str,
    file_prefix: &str,
    weight_name: &str,
) -> PathBuf {
    root.join("dist")
        .join("ttf")
        .join(dist_folder)
        .join(format!("{file_prefix}-{weight_name}.ttf"))
}

/// Pick a subset plan based on the requested strategy.
fn select_subset_plan_all(
    args: &BuildAllArgs,
    base_codepoints: &[u32],
) -> anyhow::Result<Vec<WebFontSubset>> {
    match args.strategy {
        SubsetStrategy::JisRow => Ok(build_subset_plan(
            base_codepoints.iter().copied(),
            args.extra_han_slices,
        )),
        SubsetStrategy::GoogleJapanese => build_google_japanese_subset_plan(
            base_codepoints.iter().copied(),
            &args.google_japanese_slice,
            !args.no_remaining,
            args.remaining_slices,
        ),
    }
}

fn select_subset_plan_single(
    args: &BuildSingleArgs,
    base_codepoints: &[u32],
) -> anyhow::Result<Vec<WebFontSubset>> {
    match args.strategy {
        SubsetStrategy::JisRow => Ok(build_subset_plan(
            base_codepoints.iter().copied(),
            args.extra_han_slices,
        )),
        SubsetStrategy::GoogleJapanese => build_google_japanese_subset_plan(
            base_codepoints.iter().copied(),
            &args.google_japanese_slice,
            !args.no_remaining,
            args.remaining_slices,
        ),
    }
}

/// Write a minified `all.css`-style stylesheet from a slice of face entries
/// and return the size info for the manifest.
fn write_minified_css(
    out_path: &Path,
    entries: &[&FaceEntry],
) -> anyhow::Result<crate::manifest::CssSizeInfo> {
    let mut buf = String::new();
    for entry in entries {
        let src = format!("./{}", entry.path);
        buf.push_str(&font_face_css_minified(
            &entry.family,
            entry.weight,
            &src,
            &entry.unicode_range,
        ));
    }
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating CSS parent dir {}", parent.display()))?;
    }
    std::fs::write(out_path, &buf)
        .with_context(|| format!("writing CSS to {}", out_path.display()))?;
    css_size_info(out_path)
}

// ---------------------------------------------------------------------------
// build_all
// ---------------------------------------------------------------------------

/// Multi-weight, multi-family pipeline: produces `dist/webfont/gen-interface-jp/`.
///
/// Mirrors `build_all` in `source/src/webfont/build.py` (lines ~483-620).
pub fn build_all(args: &BuildAllArgs) -> anyhow::Result<ManifestAll> {
    // 1. Resolve / clean the output directory.
    let out_dir = absolutize(&args.output)
        .with_context(|| format!("resolving output dir {}", args.output.display()))?;
    if args.clean && out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)
            .with_context(|| format!("clean: removing {}", out_dir.display()))?;
    }
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("creating output dir {}", out_dir.display()))?;

    // 2. Walk WEBFONT_FAMILIES x WEIGHTS, resolve every source TTF.
    //
    // BTreeMap keeps a deterministic iteration order — the Python uses a plain
    // dict but is order-stable since 3.7 because it inserts in this same loop
    // shape. Using BTreeMap here makes the determinism explicit.
    let mut source_paths: BTreeMap<(&'static str, u16), PathBuf> = BTreeMap::new();
    for family in WEBFONT_FAMILIES {
        for (weight, weight_name) in WEIGHTS {
            let p = source_font_path(
                &args.root,
                family.dist_folder,
                family.file_prefix,
                weight_name,
            );
            let resolved = absolutize(&p).unwrap_or(p);
            if !resolved.is_file() {
                return Err(anyhow!(
                    "Missing source font: {}\nRun: make font",
                    resolved.display()
                ));
            }
            source_paths.insert((family.key, *weight), resolved);
        }
    }

    // 3. Verify all source TTFs share a cmap, and capture that cmap.
    let path_refs: Vec<&Path> = source_paths.values().map(|p| p.as_path()).collect();
    let base_cmap = cmap::verify_matching_cmaps(&path_refs)?;
    let base_codepoints: Vec<u32> = base_cmap.into_iter().collect();

    // 4. Pick the subset plan.
    let plan = select_subset_plan_all(args, &base_codepoints)?;
    if plan.is_empty() {
        return Err(anyhow!("Subset plan is empty"));
    }

    // 5. Write `.nam` files: out_dir/nam/{i:03}.nam per slice.
    for (index, item) in plan.iter().enumerate() {
        let nam_path = out_dir.join("nam").join(format!("{index:03}.nam"));
        write_nam(&nam_path, &item.codepoints, &item.note)
            .with_context(|| format!("writing nam file {}", nam_path.display()))?;
    }

    // 6. Build the (task, file_entry, face_entry) tuples.
    //    Each task gets a stable index so the parallel workers can write back
    //    to `file_entries[i].bytes` without locking the whole vector.
    let mut tasks: Vec<(usize, PathBuf, PathBuf, Vec<u32>)> = Vec::new();
    let mut file_entries: Vec<SubsetFileEntry> = Vec::new();
    let mut face_entries: Vec<FaceEntry> = Vec::new();

    for (subset_index, item) in plan.iter().enumerate() {
        let unicode_range = format_unicode_range(item.codepoints.iter().copied());
        for family in WEBFONT_FAMILIES {
            for (weight, _) in WEIGHTS {
                let relative_path = PathBuf::from("w")
                    .join(family.key)
                    .join(weight.to_string())
                    .join(format!("{subset_index:03}.woff2"));
                let out_path = out_dir.join(&relative_path);
                let source_path = source_paths
                    .get(&(family.key, *weight))
                    .expect("source path was inserted above");

                let task_index = tasks.len();
                tasks.push((
                    task_index,
                    source_path.clone(),
                    out_path.clone(),
                    item.codepoints.clone(),
                ));

                file_entries.push(SubsetFileEntry {
                    family: family.css_family.to_string(),
                    family_key: family.key.to_string(),
                    weight: *weight,
                    subset: format!("{subset_index:03}"),
                    path: path_to_forward_slashes(&relative_path),
                    source: relative_to_root(source_path, &args.root),
                    codepoints: item.codepoints.len(),
                    unicode_range: unicode_range.clone(),
                    bytes: 0,
                });
                face_entries.push(FaceEntry {
                    family: family.css_family.to_string(),
                    family_key: family.key.to_string(),
                    weight: *weight,
                    path: path_to_forward_slashes(&relative_path),
                    unicode_range: unicode_range.clone(),
                });
            }
        }
    }

    // 7. Run the subsetter — sequential or rayon-parallel based on `jobs`.
    let total_tasks = tasks.len();
    let jobs = args.jobs.max(1);
    eprintln!("Building {total_tasks} subset WOFF2 files with {jobs} worker(s)...");

    type TaskResult = anyhow::Result<(usize, u64)>;
    let run_one = |task: &(usize, PathBuf, PathBuf, Vec<u32>)| -> TaskResult {
        let (idx, src, out, cps) = task;
        build_woff2_subset(src, out, cps).with_context(|| {
            format!(
                "subsetting {} -> {} ({} cps)",
                src.display(),
                out.display(),
                cps.len()
            )
        })?;
        let size = std::fs::metadata(out)
            .with_context(|| format!("stat WOFF2 output {}", out.display()))?
            .len();
        Ok((*idx, size))
    };

    let results: Vec<TaskResult> = if jobs == 1 {
        tasks.iter().map(run_one).collect()
    } else {
        // Build a scoped rayon thread pool with the requested job count. We
        // create a scoped pool so we don't disturb any global config the
        // caller may have set up — and so callers can run two `build_all`
        // invocations back-to-back with different `jobs` values.
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()
            .context("creating rayon thread pool")?;
        pool.install(|| tasks.par_iter().map(run_one).collect())
    };

    // Walk results in stable order so progress lines + manifest are reproducible.
    // The Python reference printed in completion order (`as_completed`); we
    // sacrifice that for byte-stable manifest output.
    for (n, res) in results.iter().enumerate() {
        let (idx, size) = match res {
            Ok(t) => *t,
            Err(e) => return Err(anyhow!("worker {} failed: {:#}", n + 1, e)),
        };
        file_entries[idx].bytes = size;
        let path_for_log = &file_entries[idx].path;
        eprintln!(
            "[{:04}/{:04}] {} {:.1} KB",
            n + 1,
            total_tasks,
            path_for_log,
            size as f64 / 1024.0
        );
    }

    // 8. Drop the legacy `index.css` if it survived a previous build.
    let legacy_index_css = out_dir.join("index.css");
    if legacy_index_css.exists() {
        let _ = std::fs::remove_file(&legacy_index_css);
    }

    // 9. Build CSS files: all.css, per-weight, per-display-weight.
    //    The Python writes the entries in the order they were appended, which
    //    is (subset_index outer, family outer, weight inner) — same here.
    let all_face_refs: Vec<&FaceEntry> = face_entries.iter().collect();
    let all_css = write_minified_css(&out_dir.join("all.css"), &all_face_refs)?;

    let mut weights_block: BTreeMap<String, crate::manifest::CssSizeInfo> = BTreeMap::new();
    let mut display_weights_block: BTreeMap<String, crate::manifest::CssSizeInfo> = BTreeMap::new();

    for (weight, _) in WEIGHTS {
        let normal: Vec<&FaceEntry> = face_entries
            .iter()
            .filter(|e| e.family_key == "normal" && e.weight == *weight)
            .collect();
        let display: Vec<&FaceEntry> = face_entries
            .iter()
            .filter(|e| e.family_key == "display" && e.weight == *weight)
            .collect();

        let normal_path = out_dir.join(weight_css_filename("normal", *weight));
        let display_path = out_dir.join(weight_css_filename("display", *weight));

        weights_block.insert(weight.to_string(), write_minified_css(&normal_path, &normal)?);
        display_weights_block
            .insert(weight.to_string(), write_minified_css(&display_path, &display)?);
    }

    // 10. Build the manifest via the in-tree manifest builder.
    let google_slice = match args.strategy {
        SubsetStrategy::GoogleJapanese => {
            Some(relative_to_root(&args.google_japanese_slice, &args.root))
        }
        SubsetStrategy::JisRow => None,
    };

    let source = ManifestSource {
        format: Some("ttf".to_string()),
        ttf: None,
        strategy: args.strategy.as_manifest_str().to_string(),
        google_japanese_slice: google_slice,
    };

    let css_block = CssEntries {
        all: all_css.clone(),
        weights: weights_block,
        display_weights: display_weights_block,
    };

    let families: Vec<FamilyEntry> = WEBFONT_FAMILIES
        .iter()
        .map(|family| FamilyEntry {
            key: family.key.to_string(),
            family: family.css_family.to_string(),
            weights: WEIGHTS.iter().map(|(w, _)| *w).collect(),
        })
        .collect();

    let font_face_count = face_entries.len();
    let subset_count = plan.len();
    let font_codepoints = base_codepoints.len();

    let manifest = ManifestAll::new(
        FAMILY_NAME,
        STYLE,
        DISPLAY,
        source,
        css_block,
        families,
        font_codepoints,
        subset_count,
        font_face_count,
        file_entries,
    );

    // 11. Write manifest.json.
    let manifest_path = out_dir.join("manifest.json");
    let json = manifest
        .to_json_string()
        .context("serialising manifest.json")?;
    std::fs::write(&manifest_path, &json)
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    eprintln!();
    eprintln!("CSS: {}", out_dir.join(&manifest.css.all.path).display());
    eprintln!("Manifest: {}", manifest_path.display());
    eprintln!(
        "Subset WOFF2: {:.2} MB",
        manifest.totals.subset_bytes as f64 / 1024.0 / 1024.0
    );
    eprintln!(
        "all.css: {:.1} KB raw, {:.1} KB gzip",
        manifest.css.all.bytes as f64 / 1024.0,
        manifest.css.all.gzip_bytes as f64 / 1024.0,
    );
    if let Some(brotli_bytes) = manifest.css.all.brotli_bytes {
        eprintln!("all.css: {:.1} KB brotli", brotli_bytes as f64 / 1024.0);
    }

    Ok(manifest)
}

// ---------------------------------------------------------------------------
// build_single (benchmark pipeline)
// ---------------------------------------------------------------------------

/// Single-Regular benchmark pipeline: produces subset chunks plus a single
/// full WOFF2 baseline. Used by `benchmark.mjs`, not by the public release.
///
/// Mirrors `build` in `source/src/webfont/build.py` (lines ~623-714).
pub fn build_single(args: &BuildSingleArgs) -> anyhow::Result<ManifestSingle> {
    let src_ttf = absolutize(&args.ttf)
        .with_context(|| format!("resolving source TTF {}", args.ttf.display()))?;
    let out_dir = absolutize(&args.output)
        .with_context(|| format!("resolving output dir {}", args.output.display()))?;

    if !src_ttf.is_file() {
        return Err(anyhow!(
            "Missing source TTF: {}\nRun: make font",
            src_ttf.display()
        ));
    }

    if args.clean && out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)
            .with_context(|| format!("clean: removing {}", out_dir.display()))?;
    }
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("creating output dir {}", out_dir.display()))?;

    // Read the source font's cmap so the subset plan can intersect with it.
    let cmap_set = cmap::read_cmap_codepoints(&src_ttf)?;
    let font_codepoints: Vec<u32> = cmap_set.into_iter().collect();

    let plan = select_subset_plan_single(args, &font_codepoints)?;

    // 1. Full WOFF2 baseline.
    let full_relative = PathBuf::from("full").join("GenInterfaceJP-Regular.woff2");
    let full_out = out_dir.join(&full_relative);
    build_full_woff2(&src_ttf, &full_out)
        .with_context(|| format!("building full WOFF2 at {}", full_out.display()))?;
    let full_bytes = std::fs::metadata(&full_out)
        .with_context(|| format!("stat {}", full_out.display()))?
        .len();
    let full_entry = FullFileEntry {
        path: path_to_forward_slashes(&full_relative),
        bytes: full_bytes,
    };

    // 2. Per-subset WOFF2 chunks + matching `.nam` files.
    let mut subset_entries: Vec<SingleSubsetEntry> = Vec::new();
    let mut covered: BTreeSet<u32> = BTreeSet::new();

    for (i, item) in plan.iter().enumerate() {
        let filename = format!("GenInterfaceJP-Regular-{}.woff2", item.name);
        let relative_path = PathBuf::from("subsets").join(&filename);
        let out_path = out_dir.join(&relative_path);
        build_woff2_subset(&src_ttf, &out_path, &item.codepoints).with_context(|| {
            format!(
                "subsetting {} -> {} ({} cps)",
                src_ttf.display(),
                out_path.display(),
                item.codepoints.len()
            )
        })?;
        let nam_path = out_dir.join("nam").join(format!("{}.nam", item.name));
        write_nam(&nam_path, &item.codepoints, &item.note)
            .with_context(|| format!("writing nam {}", nam_path.display()))?;

        let size = std::fs::metadata(&out_path)
            .with_context(|| format!("stat {}", out_path.display()))?
            .len();

        eprintln!(
            "[{:03}/{:03}] {}: {} cps, {:.1} KB",
            i + 1,
            plan.len(),
            item.name,
            item.codepoints.len(),
            size as f64 / 1024.0,
        );

        for cp in &item.codepoints {
            covered.insert(*cp);
        }

        subset_entries.push(SingleSubsetEntry {
            name: item.name.clone(),
            path: path_to_forward_slashes(&relative_path),
            nam: format!("nam/{}.nam", item.name),
            bytes: size,
            codepoints: item.codepoints.len(),
            unicode_range: format_unicode_range(item.codepoints.iter().copied()),
            note: item.note.clone(),
        });
    }

    // 3. Two stylesheets: chunked subset + full single-file fallback.
    write_single_stylesheets(&out_dir, &subset_entries, &full_entry)?;

    // 4. Manifest.
    let google_slice = match args.strategy {
        SubsetStrategy::GoogleJapanese => {
            Some(relative_to_root(&args.google_japanese_slice, &args.root))
        }
        SubsetStrategy::JisRow => None,
    };

    let source = ManifestSource {
        format: None,
        ttf: Some(relative_to_root(&src_ttf, &args.root)),
        strategy: args.strategy.as_manifest_str().to_string(),
        google_japanese_slice: google_slice,
    };
    let css_block = CssSingle {
        subset: "gen-interface-jp-regular.css".to_string(),
        full: "gen-interface-jp-regular-full.css".to_string(),
    };

    let manifest = ManifestSingle::new(
        FAMILY_NAME,
        STYLE,
        WEIGHT_DEFAULT,
        DISPLAY,
        source,
        css_block,
        full_entry,
        subset_entries,
        font_codepoints.len(),
        covered.len(),
    );

    let manifest_path = out_dir.join("manifest.json");
    let json = manifest
        .to_json_string()
        .context("serialising manifest.json")?;
    std::fs::write(&manifest_path, &json)
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    eprintln!();
    eprintln!(
        "CSS: {}",
        out_dir.join("gen-interface-jp-regular.css").display()
    );
    eprintln!(
        "Full CSS: {}",
        out_dir.join("gen-interface-jp-regular-full.css").display()
    );
    eprintln!("Manifest: {}", manifest_path.display());

    Ok(manifest)
}

/// Write the two single-Regular stylesheets used by the benchmark.
///
/// Mirrors `write_css` in `source/src/webfont/build.py` (lines ~462-481).
fn write_single_stylesheets(
    out_dir: &Path,
    subset_entries: &[SingleSubsetEntry],
    full_entry: &FullFileEntry,
) -> anyhow::Result<()> {
    // Subset stylesheet: pretty-printed @font-face per chunk.
    //
    // The Python builds a list of lines and joins on `\n`. Each subset
    // contributes the multi-line @font-face block plus an empty separator
    // line, which manifests as a blank line between blocks.
    let mut subset_lines: Vec<String> = Vec::new();
    subset_lines.push("/* Generated by webfont.build. */".to_string());
    subset_lines.push(
        "/* Load with: <link rel=\"stylesheet\" href=\"/webfonts/gen-interface-jp/regular/gen-interface-jp-regular.css\"> */"
            .to_string(),
    );
    subset_lines.push(String::new());
    for entry in subset_entries {
        let src = format!("./{}", entry.path);
        subset_lines.push(font_face_css(
            FAMILY_NAME,
            WEIGHT_DEFAULT,
            &src,
            Some(&entry.unicode_range),
        ));
        subset_lines.push(String::new());
    }
    std::fs::write(
        out_dir.join("gen-interface-jp-regular.css"),
        subset_lines.join("\n"),
    )
    .context("writing gen-interface-jp-regular.css")?;

    // Full-file stylesheet: header + a single @font-face with no unicode-range.
    let mut full_lines: Vec<String> = Vec::new();
    full_lines.push("/* Generated by webfont.build. */".to_string());
    full_lines.push("/* Full, unsubsetted WOFF2 fallback/benchmark stylesheet. */".to_string());
    full_lines.push(String::new());
    let full_src = format!("./{}", full_entry.path);
    full_lines.push(font_face_css(FAMILY_NAME, WEIGHT_DEFAULT, &full_src, None));
    full_lines.push(String::new());
    std::fs::write(
        out_dir.join("gen-interface-jp-regular-full.css"),
        full_lines.join("\n"),
    )
    .context("writing gen-interface-jp-regular-full.css")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
//
// These are smoke tests only. End-to-end testing requires real source TTFs
// and the WOFF2 subsetter (still TODO(impl) in `subset.rs`). Both pipelines
// are exercised at the integration-test level under `crates/gen-webfont/tests/`.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_manifest_strings_match_python_wire_format() {
        assert_eq!(SubsetStrategy::JisRow.as_manifest_str(), "jis-row");
        assert_eq!(
            SubsetStrategy::GoogleJapanese.as_manifest_str(),
            "google-japanese"
        );
    }

    #[test]
    fn path_to_forward_slashes_handles_simple_relative_path() {
        let p = Path::new("w").join("normal").join("400").join("000.woff2");
        assert_eq!(path_to_forward_slashes(&p), "w/normal/400/000.woff2");
    }

    #[test]
    fn relative_to_root_returns_relative_when_inside() {
        // Use the workspace's own crates dir so symlink-canonicalisation
        // (e.g. macOS `/tmp` -> `/private/tmp`) doesn't trip the equality.
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let inside = root.join("src").join("build_runner.rs");
        let got = relative_to_root(&inside, &root);
        assert_eq!(got, "src/build_runner.rs");
    }

    #[test]
    fn build_all_args_are_constructible() {
        let _ = BuildAllArgs {
            root: PathBuf::from("/"),
            output: PathBuf::from("/tmp/out"),
            clean: false,
            strategy: SubsetStrategy::JisRow,
            jobs: 1,
            extra_han_slices: 24,
            no_remaining: false,
            remaining_slices: 8,
            google_japanese_slice: PathBuf::from("/dev/null"),
        };
    }

    #[test]
    fn build_single_args_are_constructible() {
        let _ = BuildSingleArgs {
            root: PathBuf::from("/"),
            ttf: PathBuf::from("/dev/null"),
            output: PathBuf::from("/tmp/out"),
            clean: false,
            strategy: SubsetStrategy::GoogleJapanese,
            extra_han_slices: 24,
            no_remaining: true,
            remaining_slices: 8,
            google_japanese_slice: PathBuf::from("/dev/null"),
        };
    }
}
