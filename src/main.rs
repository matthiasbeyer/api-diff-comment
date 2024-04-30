use std::io::Write;
use std::path::{Path, PathBuf};

use argh::FromArgs;
use miette::Context;
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

    /// path to print the output to. If not provided, output will be printed to stdout
    #[argh(option, short = 'o', long = "output")]
    output: Option<PathBuf>,
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

const TEMPLATE_NAME: &str = "template";

#[derive(serde::Serialize)]
struct TemplateData {
    added: Vec<String>,
    removed: Vec<String>,
    changed: Vec<ChangedItem>,
}

#[derive(serde::Serialize)]
struct ChangedItem {
    old: String,
    new: String,
}

fn main() -> Result<(), miette::Error> {
    let args: Args = argh::from_env();

    tracing_subscriber::fmt::fmt()
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_level(args.verbose)
        .with_max_level(
            args.verbose
                .then(|| tracing::Level::DEBUG)
                .unwrap_or(tracing::Level::INFO),
        )
        .init();

    tracing::debug!(?args, "Arguments parsed");

    let handlebars = {
        let mut handlebars = handlebars::Handlebars::new();
        let template = std::fs::read_to_string(&args.template)
            .into_diagnostic()
            .context("reading template file")?;

        handlebars
            .register_template_string(TEMPLATE_NAME, template)
            .into_diagnostic()
            .context("registration of template in the handlebars runtime")?;

        handlebars
    };

    let output_file = args
        .output
        .as_ref()
        .map(|path| {
            std::fs::OpenOptions::new()
                .create_new(true)
                .truncate(true)
                .open(path)
                .into_diagnostic()
                .context("opening output file")
        })
        .transpose()?;

    let tempdir = args
        .tempdir
        .as_ref()
        .map(Tempdir::Provided)
        .map(Ok)
        .unwrap_or_else(|| tempfile::tempdir().map(Tempdir::Generated))
        .into_diagnostic()?;

    tracing::info!(path = %tempdir.path().display(), "Tempdir path found!");

    let cwd = std::env::current_dir().into_diagnostic()?;

    let base_wt_path = {
        let mut dir = tempdir.path().to_path_buf();
        dir.push("base");
        dir
    };
    let target_wt_path = {
        let mut dir = tempdir.path().to_path_buf();
        dir.push("target");
        dir
    };

    let base = args.base.clone();
    let cwd_clone = cwd.clone();
    let base_doc = std::thread::spawn(move || {
        build_pubapi_for_reference(&cwd_clone, &base, &base_wt_path)
    });

    let target = args.target.clone();
    let target_doc = std::thread::spawn(move || {
        build_pubapi_for_reference(&cwd, &target, &target_wt_path)
    });

    let base_doc = base_doc
        .join()
        .map_err(|_| miette::miette!("Failed to join thread"))
        .and_then(std::convert::identity)?;

    let target_doc = target_doc
        .join()
        .map_err(|_| miette::miette!("Failed to join thread"))
        .and_then(std::convert::identity)?;

    let diff = public_api::diff::PublicApiDiff::between(base_doc, target_doc);

    let template_data = {
        TemplateData {
            added: diff.added.into_iter().map(|itm| itm.to_string()).collect(),
            removed: diff
                .removed
                .into_iter()
                .map(|itm| itm.to_string())
                .collect(),
            changed: diff
                .changed
                .into_iter()
                .map(|itm| ChangedItem {
                    old: itm.old.to_string(),
                    new: itm.new.to_string(),
                })
                .collect(),
        }
    };

    let rendered = handlebars
        .render(TEMPLATE_NAME, &template_data)
        .into_diagnostic()
        .context("rendering template")?;

    if let Some(mut output) = output_file {
        output
            .write_all(rendered.as_bytes())
            .into_diagnostic()
            .context("writing output")?;
    } else {
        let stdout = std::io::stdout();
        let mut outlock = stdout.lock();
        outlock
            .write_all(rendered.as_bytes())
            .into_diagnostic()
            .context("writing output")?;
    }

    Ok(())
}

fn build_pubapi_for_reference(
    cwd: &Path,
    reference: &str,
    wt_path: &Path,
) -> Result<public_api::PublicApi, miette::Error> {
    let wt_path = wt_path
        .to_str()
        .ok_or_else(|| miette::miette!("Path is not UTF8: {}", wt_path.display()))?;

    {
        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(cwd);
        cmd.args(["worktree", "add", wt_path, reference]);
        let cmdout = cmd
            .spawn()
            .map_err(|e| miette::miette!("Failed to spawn git-worktree: {}", e))?
            .wait()
            .map_err(|e| miette::miette!("Failed to run git-worktree: {}", e))?;

        if !cmdout.success() {
            miette::bail!("git-worktree {} {} failed", wt_path, reference)
        }
    }
    {
        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(cwd);
        cmd.args(["checkout", reference]);
        let cmdout = cmd
            .spawn()
            .map_err(|e| miette::miette!("Failed to spawn git-checkout: {}", e))?
            .wait()
            .map_err(|e| miette::miette!("Failed to run git-checkout: {}", e))?;

        if !cmdout.success() {
            miette::bail!("git-checkout {} failed", reference)
        }
    }

    let build_json = || {
        rustdoc_json::Builder::default()
            .toolchain("nightly")
            .manifest_path(wt_path)
            .build()
    };

    let json = build_json();

    {
        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(cwd);
        cmd.args(["worktree", "remove", "-f", wt_path]);
        let cmdout = cmd
            .spawn()
            .map_err(|e| miette::miette!("Failed to spawn git-worktree: {}", e))?
            .wait()
            .map_err(|e| miette::miette!("Failed to run git-worktree: {}", e))?;

        if !cmdout.success() {
            miette::bail!("git-worktree remove -f {} failed", wt_path)
        }
    }

    let json = json.into_diagnostic()?;

    let doc = public_api::Builder::from_rustdoc_json(json)
        .build()
        .into_diagnostic()?;
    Ok(doc)
}
