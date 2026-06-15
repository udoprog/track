use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};

const LANGUAGES_DEFAULT: &str = "crates/iso639/data/iso-639-3.tab";
const COUNTRIES_DEFAULT: &str = "crates/iso3166/data/all.csv";

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
    }

    Ok(())
}

fn download_languages(output: Option<PathBuf>) -> Result<()> {
    let output = output.unwrap_or_else(|| PathBuf::from(LANGUAGES_DEFAULT));
    let body = update_languages::download_table()?;
    write_file(&output, &body)
}

fn download_countries(output: Option<PathBuf>) -> Result<()> {
    let output = output.unwrap_or_else(|| PathBuf::from(COUNTRIES_DEFAULT));
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

fn write_file(output: &Path, body: &str) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(output, body).with_context(|| anyhow!("Writing {}", output.display()))?;
    eprintln!("Wrote {}", output.display());
    Ok(())
}
