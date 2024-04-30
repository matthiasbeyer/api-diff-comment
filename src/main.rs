use std::path::{Path, PathBuf};

use argh::FromArgs;
use miette::IntoDiagnostic;

/// Arguments
#[derive(Debug, FromArgs)]
struct Args {
    /// whether to be verbose in logging
    #[argh(switch, short = 'v', long = "verbose")]
    verbose: bool,

    /// base ref to compare against
    #[argh(positional)]
    base: String,

    /// target ref to compare
    #[argh(positional)]
    target: String,

    /// template to interpolate the diff data into
    #[argh(positional)]
    template: PathBuf,

    /// directory to create worktrees in
    #[argh(option, long = "tempdir")]
    tempdir: Option<PathBuf>,
}

enum Tempdir<'a> {
    Provided(&'a PathBuf),
    Generated(tempfile::TempDir),
}

impl<'a> Tempdir<'a> {
    pub fn path(&self) -> &Path {
        match self {
            Tempdir::Provided(p) => p.as_ref(),
            Tempdir::Generated(td) => td.path(),
        }
    }
}

fn main() -> Result<(), miette::Error> {
    tracing_subscriber::fmt::init();

    let args: Args = argh::from_env();
    tracing::debug!(?args, "Arguments parsed");

    let tempdir = args
        .tempdir
        .as_ref()
        .map(Tempdir::Provided)
        .map(Ok)
        .unwrap_or_else(|| tempfile::tempdir().map(Tempdir::Generated))
        .into_diagnostic()?;

    tracing::info!(path = %tempdir.path().display(), "Tempdir path found!");

    Ok(())
}
