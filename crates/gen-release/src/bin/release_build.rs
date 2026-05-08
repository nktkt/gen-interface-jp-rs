use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about = "Prepare release artifacts for Gen Interface JP.")]
struct Args {
    /// Release version. Defaults to `GITHUB_REF_NAME` or workspace Cargo.toml.
    #[arg(long)]
    version: Option<String>,

    /// GitHub owner/repo for release URLs.
    #[arg(long, default_value_t = default_repository())]
    repository: String,

    /// Release output directory.
    #[arg(long, default_value = "../source/dist/release")]
    output: PathBuf,

    /// Built Gen Interface JP webfont package directory.
    #[arg(long, default_value = "../source/dist/webfont/gen-interface-jp")]
    webfont_source: PathBuf,
}

fn default_repository() -> String {
    std::env::var("GITHUB_REPOSITORY")
        .unwrap_or_else(|_| gen_release::DEFAULT_REPOSITORY.to_string())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let workspace_root = std::env::current_dir()?;
    let source_root = workspace_root.join("../source");

    let build_args = gen_release::BuildReleaseArgs {
        version: args.version,
        repository: args.repository,
        output: args.output,
        webfont_source: args.webfont_source,
        workspace_root,
        source_root,
    };

    gen_release::build_release(&build_args)?;

    Ok(())
}
