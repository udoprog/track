use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};

// Default paths are relative to the workspace root so they resolve regardless of the
// directory the command is invoked from (see `workspace_root`).
const LANGUAGES_DEFAULT: &str = "crates/iso639/data/iso-639-3.tab";
const COUNTRIES_DEFAULT: &str = "crates/iso3166/data/all.csv";
const FLAGS_DIR_DEFAULT: &str = "3rdparty/flag-icons/flags/4x3";
const FLAGS_SCSS_DEFAULT: &str = "crates/frontend/style/_flags_generated.scss";
const ICONS_DIR_DEFAULT: &str = "3rdparty/heroicons/optimized/24/solid";
const ICONS_SCSS_DEFAULT: &str = "crates/frontend/style/_icons_generated.scss";

/// The workspace root, derived from this crate's manifest dir (`<root>/crates/tools`) at
/// compile time, so default paths don't depend on the current working directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tools crate manifest dir has a workspace root two levels up")
        .to_path_buf()
}

/// Refresh the bundled ISO datasets and generated modules.
#[derive(Parser)]
#[command(about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download the ISO 639-3 language dataset.
    DownloadLanguages {
        /// Output file (defaults to the bundled language data).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Download the ISO 3166-1 country dataset.
    DownloadCountries {
        /// Output file (defaults to the bundled country data).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Download every dataset to its default location.
    DownloadAll,
    /// Generate the standalone language module (prints to stdout unless `--output` is given).
    GenerateLanguages {
        /// Emit the data-only module (no type definitions).
        #[arg(long)]
        data_module: bool,
        /// Output file (defaults to stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate the flag-name Sass list from the bundled flag SVGs.
    GenerateFlags {
        /// Directory of `4x3` flag SVGs (defaults to the bundled submodule).
        #[arg(long)]
        flags_dir: Option<PathBuf>,
        /// Output file (defaults to the generated flags partial).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate the icon-name Sass list from the bundled icon SVGs.
    GenerateIcons {
        /// Directory of icon SVGs (defaults to the bundled submodule).
        #[arg(long)]
        icons_dir: Option<PathBuf>,
        /// Output file (defaults to the generated icons partial).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::DownloadLanguages { output } => download_languages(output)?,
        Command::DownloadCountries { output } => download_countries(output)?,
        Command::DownloadAll => {
            download_languages(None)?;
            download_countries(None)?;
        }
        Command::GenerateLanguages {
            data_module,
            output,
        } => generate_languages(data_module, output)?,
        Command::GenerateFlags { flags_dir, output } => generate_flags(flags_dir, output)?,
        Command::GenerateIcons { icons_dir, output } => generate_icons(icons_dir, output)?,
    }

    Ok(())
}

fn download_languages(output: Option<PathBuf>) -> Result<()> {
    let output = output.unwrap_or_else(|| workspace_root().join(LANGUAGES_DEFAULT));
    let body = update_languages::download_table()?;
    write_file(&output, &body)
}

fn download_countries(output: Option<PathBuf>) -> Result<()> {
    let output = output.unwrap_or_else(|| workspace_root().join(COUNTRIES_DEFAULT));
    let body = update_languages::download_countries()?;
    write_file(&output, &body)
}

fn generate_languages(data_module: bool, output: Option<PathBuf>) -> Result<()> {
    let module = if data_module {
        update_languages::generate_data_module()?
    } else {
        update_languages::generate_standalone_module()?
    };

    match output {
        Some(path) => {
            fs::write(&path, module)
                .with_context(|| anyhow!("Writing generated output to {}", path.display()))?;
            eprintln!("Wrote language sets to {}", path.display());
        }
        None => print!("{module}"),
    }

    Ok(())
}

fn generate_flags(flags_dir: Option<PathBuf>, output: Option<PathBuf>) -> Result<()> {
    let flags_dir = flags_dir.unwrap_or_else(|| workspace_root().join(FLAGS_DIR_DEFAULT));
    let output = output.unwrap_or_else(|| workspace_root().join(FLAGS_SCSS_DEFAULT));
    let scss = update_languages::generate_svg_names_scss(&flags_dir)?;
    write_file(&output, &scss)
}

fn generate_icons(icons_dir: Option<PathBuf>, output: Option<PathBuf>) -> Result<()> {
    let icons_dir = icons_dir.unwrap_or_else(|| workspace_root().join(ICONS_DIR_DEFAULT));
    let output = output.unwrap_or_else(|| workspace_root().join(ICONS_SCSS_DEFAULT));
    let scss = update_languages::generate_svg_names_scss(&icons_dir)?;
    write_file(&output, &scss)
}

fn write_file(output: &Path, body: &str) -> Result<()> {
    // Skip rewriting unchanged content so we don't bump the mtime (which would retrigger
    // Trunk's file watcher into a rebuild loop) or create needless git churn.
    if fs::read_to_string(output).is_ok_and(|existing| existing == body) {
        eprintln!("Unchanged {}", output.display());
        return Ok(());
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(output, body).with_context(|| anyhow!("Writing {}", output.display()))?;
    eprintln!("Wrote {}", output.display());
    Ok(())
}
