//! CLI entrypoint for the `gen-font` build pipeline.
//!
//! Mirrors `python -m font.build` from `../source/`. Walks the
//! `(family x weight)` matrix and invokes [`gen_font::build_one`] for each
//! cell, printing progress in the same format as the Python driver so the
//! two outputs can be diffed by eye.
//!
//! Usage:
//! ```text
//! font_build                       # everything (all families x all weights)
//! font_build normal                # all weights of the "normal" family
//! font_build normal Regular Bold   # specific weights of one family
//! font_build all 400 700           # by weight num, both families
//! ```
//!
//! The first positional may be a family key (`normal` / `display` / `all`);
//! if it does not match a known family it is treated as a weight filter and
//! the family defaults to `all`. Remaining positionals match weights by
//! `weight_name` (e.g. `"Regular"`) or stringified `weight_num` (e.g. `"400"`).

use anyhow::{anyhow, Result};
use clap::Parser;

use gen_font::build::BuildPaths;
use gen_font::{build_one, FamilyConfig, WeightSpec, FAMILIES, WEIGHTS};

/// Drive the family/weight build matrix.
///
/// Positional parsing is lenient: the first arg may be a family key
/// (`normal`, `display`, `all`) or a weight filter. Everything after the
/// (optional) family is treated as a weight filter.
#[derive(Parser, Debug)]
#[command(
    name = "font_build",
    about = "Build Gen Interface JP font families (family x weight matrix)."
)]
struct Args {
    /// Optional family key followed by zero-or-more weight filters.
    ///
    /// Examples:
    ///   `normal`              -- all weights of "normal"
    ///   `normal Regular Bold` -- two weights of "normal"
    ///   `all 400 700`         -- two weights of every family
    ///   `Regular`             -- one weight of every family (family inferred as "all")
    #[arg(value_name = "FAMILY_OR_WEIGHT")]
    positional: Vec<String>,
}

/// Resolve `(families_to_build, weights_to_build)` from the raw positional
/// arguments. The first positional is consumed as a family key only if it
/// matches one of `FAMILIES` or the literal `"all"`; otherwise it falls
/// through and is reinterpreted as a weight filter.
fn resolve_selection(
    positional: &[String],
) -> Result<(Vec<&'static FamilyConfig>, Vec<&'static WeightSpec>)> {
    let mut args: &[String] = positional;

    // Default: every family, every weight.
    let mut families: Vec<&'static FamilyConfig> = FAMILIES.iter().collect();

    if let Some(first) = args.first() {
        let lower = first.to_lowercase();
        if lower == "all" {
            args = &args[1..];
        } else if let Some(family) = FamilyConfig::lookup(&lower) {
            families = vec![family];
            args = &args[1..];
        }
    }

    let weights: Vec<&'static WeightSpec> = if args.is_empty() {
        WEIGHTS.iter().collect()
    } else {
        let filtered: Vec<&'static WeightSpec> = WEIGHTS
            .iter()
            .filter(|w| {
                args.iter().any(|raw| {
                    let s = raw.trim();
                    s == w.weight_name || s == w.weight_num.to_string()
                })
            })
            .collect();
        if filtered.is_empty() {
            let available: Vec<&str> = WEIGHTS.iter().map(|w| w.weight_name).collect();
            return Err(anyhow!(
                "No matching weights. Available: {:?}",
                available
            ));
        }
        filtered
    };

    Ok((families, weights))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let paths = BuildPaths::default_for_rust_workspace();
    let (families, weights) = resolve_selection(&args.positional)?;

    let bar = "=".repeat(60);

    for family in &families {
        let total = weights.len();
        println!("\n{bar}");
        println!("  {}  (tracking +{})", family.family_name, family.tracking);
        println!("{bar}");

        for (i, weight) in weights.iter().enumerate() {
            let idx = i + 1;
            println!(
                "\n[{idx}/{total}] {} {} ({})...",
                family.family_name, weight.weight_name, weight.weight_num
            );
            let result = build_one(&paths, family, weight)?;
            println!("  -> {}", result.font_path.display());
        }

        println!("\n  Done. {total} weight(s) of {}", family.family_name);
    }

    println!("\nAll done. Output in {}", paths.dist_ttf.display());
    Ok(())
}
