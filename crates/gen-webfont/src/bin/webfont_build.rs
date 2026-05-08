//! CLI entrypoint for `gen-webfont`.
//!
//! Replaces `python3 -m webfont.build` (see
//! `source/src/webfont/build.py`) and drives subset generation either for a
//! single Regular weight or for the full Text + Display matrix across every
//! weight and CSS entrypoint.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use gen_webfont::build_runner::{self, BuildAllArgs, BuildSingleArgs, SubsetStrategy};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Build unicode-range web font subsets for Gen Interface JP."
)]
struct Args {
    /// Build Text + Display subset WOFF2 for all weights and CSS entrypoints.
    #[arg(long)]
    all: bool,
    /// Source Gen Interface JP Regular TTF.
    #[arg(
        long,
        default_value = "../source/dist/ttf/Gen Interface JP/GenInterfaceJP-Regular.ttf"
    )]
    ttf: PathBuf,
    /// Output directory.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Parallel workers for --all subset generation.
    #[arg(long, default_value_t = num_jobs_default())]
    jobs: usize,
    /// Subset partitioning strategy.
    #[arg(long, value_enum, default_value_t = StrategyArg::GoogleJapanese)]
    strategy: StrategyArg,
    /// googlefonts/nam-files `slices/japanese_default.txt`
    #[arg(
        long,
        default_value = "../source/vendor/nam-files/slices/japanese_default.txt"
    )]
    google_japanese_slice: PathBuf,
    /// Do not add extra subsets for cmap codepoints outside the selected strategy.
    #[arg(long)]
    no_remaining: bool,
    /// Number of extra subsets for codepoints outside the selected strategy.
    #[arg(long, default_value_t = 8)]
    remaining_slices: usize,
    /// Slices for CJK codepoints outside JIS X 0208.
    #[arg(long, default_value_t = 24)]
    extra_han_slices: usize,
    /// Remove the output directory before building.
    #[arg(long)]
    clean: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum StrategyArg {
    GoogleJapanese,
    JisRow,
}

impl From<StrategyArg> for SubsetStrategy {
    fn from(value: StrategyArg) -> Self {
        match value {
            StrategyArg::GoogleJapanese => SubsetStrategy::GoogleJapanese,
            StrategyArg::JisRow => SubsetStrategy::JisRow,
        }
    }
}

fn num_jobs_default() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |n| n.get().min(4))
        .max(1)
}

const DEFAULT_SINGLE_OUT: &str = "../source/dist/webfont/GenInterfaceJP-Regular";
const DEFAULT_ALL_OUT: &str = "../source/dist/webfont/gen-interface-jp";
const DEFAULT_ROOT: &str = "../source";

fn main() -> Result<()> {
    let args = Args::parse();

    let strategy: SubsetStrategy = args.strategy.clone().into();
    let root = PathBuf::from(DEFAULT_ROOT);

    if args.all {
        let output = args
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_ALL_OUT));
        let build_args = BuildAllArgs {
            output,
            root,
            jobs: args.jobs,
            strategy,
            google_japanese_slice: args.google_japanese_slice,
            no_remaining: args.no_remaining,
            remaining_slices: args.remaining_slices,
            extra_han_slices: args.extra_han_slices,
            clean: args.clean,
        };
        let _ = args.ttf; // BuildAllArgs uses dist/ttf paths internally; ttf flag is single-Regular only.
        build_runner::build_all(&build_args)?;
    } else {
        let output = args
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SINGLE_OUT));
        let build_args = BuildSingleArgs {
            ttf: args.ttf,
            output,
            root,
            strategy,
            google_japanese_slice: args.google_japanese_slice,
            no_remaining: args.no_remaining,
            remaining_slices: args.remaining_slices,
            extra_han_slices: args.extra_han_slices,
            clean: args.clean,
        };
        build_runner::build_single(&build_args)?;
    }

    Ok(())
}
