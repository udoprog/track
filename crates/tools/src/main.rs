use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};

mod generate;

// 3rdparty submodules and the path probed to confirm each is initialized.
const FLAG_ICONS_DIR: &str = "3rdparty/flag-icons/flags/4x3";
const HEROICONS_DIR: &str = "3rdparty/heroicons/optimized/24/solid";

// Bundled ISO datasets (committed inputs).
const ISO639_TAB: &str = "crates/iso639/data/iso-639-3.tab";
const ISO639_TO_3166: &str = "crates/iso639/data/to-3166.csv";
const ISO3166_CSV: &str = "crates/iso3166/data/all.csv";

// Generated, committed source files (formerly produced by each crate's build.rs).
const ISO639_GENERATED: &str = "crates/iso639/src/generated.rs";
const ISO3166_GENERATED: &str = "crates/iso3166/src/generated.rs";

// Generated, committed Sass name lists.
const FLAGS_SCSS: &str = "crates/frontend/style/_flags_generated.scss";
const ICONS_SCSS: &str = "crates/frontend/style/_icons_generated.scss";

/// Refresh the bundled datasets and the source/style files generated from them. With no
/// subcommand it regenerates everything from the committed inputs.
#[derive(Parser)]
#[command(about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(long)]
    mapping_path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Regenerate the flag/icon Sass name lists (the Trunk pre_build entry point).
    Scss,
    /// Regenerate the `iso639` source modules from the committed datasets.
    Iso639,
    /// Regenerate the `iso3166` source module from the committed dataset.
    Iso3166,
    /// Download the ISO 639-3 dataset into the bundled data file.
    DownloadLanguages,
    /// Download the ISO 3166-1 dataset into the bundled data file.
    DownloadCountries,
    /// Download every dataset into its bundled data file.
    DownloadAll,
}

fn main() -> Result<()> {
    let root = workspace_root();
    let cli = Cli::parse();

    match &cli.command {
        None => {
            // Default: bring every generated artifact up to date.
            generate_iso639(&root, cli.mapping_path.as_deref())?;
            generate_iso3166(&root)?;
            generate_scss(&root)?;
        }
        Some(Command::Scss) => generate_scss(&root)?,
        Some(Command::Iso639) => generate_iso639(&root, cli.mapping_path.as_deref())?,
        Some(Command::Iso3166) => generate_iso3166(&root)?,
        Some(Command::DownloadLanguages) => download_languages(&root)?,
        Some(Command::DownloadCountries) => download_countries(&root)?,
        Some(Command::DownloadAll) => {
            download_languages(&root)?;
            download_countries(&root)?;
        }
    }

    Ok(())
}

/// The workspace root, derived from this crate's manifest dir (`<root>/crates/tools`) at
/// compile time, so paths don't depend on the current working directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tools crate manifest dir has a workspace root two levels up")
        .to_path_buf()
}

/// Confirm a 3rdparty submodule is initialized by probing a path that only exists once it
/// has been checked out.
fn ensure_submodule(root: &Path, probe: &str) -> Result<PathBuf> {
    let path = root.join(probe);

    if !path.is_dir() {
        bail!(
            "3rdparty submodule is not initialized (missing {}).\n\
             Run: git submodule update --init --recursive",
            path.display()
        );
    }

    Ok(path)
}

fn generate_scss(root: &Path) -> Result<()> {
    let flags_dir = ensure_submodule(root, FLAG_ICONS_DIR)?;
    let icons_dir = ensure_submodule(root, HEROICONS_DIR)?;

    write_file(
        &root.join(FLAGS_SCSS),
        &generate::svg_names_scss(&flags_dir, true)?,
    )?;

    write_file(
        &root.join(ICONS_SCSS),
        &generate::svg_names_scss(&icons_dir, false)?,
    )?;

    Ok(())
}

fn generate_iso639(root: &Path, mapping_path: Option<&Path>) -> Result<()> {
    let flags_dir = ensure_submodule(root, FLAG_ICONS_DIR)?;

    let tab = read_input(root, ISO639_TAB)?;
    let to_3166 = read_input(root, ISO639_TO_3166)?;

    let (iso639_generated, mapping) = generate::iso639_module(&tab, &to_3166, &flags_dir)?;

    write_file(&root.join(ISO639_GENERATED), &iso639_generated)?;

    if let Some(path) = mapping_path {
        let mut o = csv::Writer::from_writer(File::create(path)?);

        for m in mapping {
            o.serialize(m)?;
        }
    }

    Ok(())
}

fn generate_iso3166(root: &Path) -> Result<()> {
    let flags_dir = ensure_submodule(root, FLAG_ICONS_DIR)?;

    let csv = read_input(root, ISO3166_CSV)?;
    write_file(
        &root.join(ISO3166_GENERATED),
        &generate::iso3166_module(&csv, &flags_dir)?,
    )?;
    Ok(())
}

fn download_languages(root: &Path) -> Result<()> {
    write_file(&root.join(ISO639_TAB), &generate::download_table()?)
}

fn download_countries(root: &Path) -> Result<()> {
    write_file(&root.join(ISO3166_CSV), &generate::download_countries()?)
}

fn read_input(root: &Path, relative: &str) -> Result<String> {
    let path = root.join(relative);
    fs::read_to_string(&path).with_context(|| anyhow!("Reading {}", path.display()))
}

fn write_file(output: &Path, body: &str) -> Result<()> {
    // Skip rewriting unchanged content so we don't bump the mtime (which would retrigger
    // Trunk's file watcher into a rebuild loop) or create needless git churn.
    if fs::read_to_string(output).is_ok_and(|existing| existing == body) {
        return Ok(());
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(output, body).with_context(|| anyhow!("Writing {}", output.display()))?;
    println!("Wrote {}", output.display());
    Ok(())
}
