use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};

mod generate;
mod vendor;

// Vendored SVG sets; see `vendor.rs`.
const FLAG_ICONS_DIR: &str = "crates/frontend/flags";
const HEROICONS_DIR: &str = "crates/frontend/icons";

// Bundled ISO datasets (committed inputs).
const ISO639_TAB: &str = "crates/iso639/data/iso-639-3.tab";
const ISO639_TO_3166: &str = "crates/iso639/data/to-3166.csv";
const ISO3166_CSV: &str = "crates/iso3166/data/all.csv";
const LOCALES_JSON: &str = "crates/locales/data/locales.json";

// Generated, committed source files (formerly produced by each crate's build.rs).
const ISO639_GENERATED: &str = "crates/iso639/src/generated.rs";
const ISO3166_GENERATED: &str = "crates/iso3166/src/generated.rs";
const LOCALES_GENERATED: &str = "crates/locales/src/generated.rs";

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
    /// Regenerate the `locales` source module from the committed dataset.
    Locales,
    /// Download the ISO 639-3 dataset into the bundled data file.
    DownloadLanguages,
    /// Download the ISO 3166-1 dataset into the bundled data file.
    DownloadCountries,
    /// Download the SimpleLocalize locales dataset into the bundled data file.
    DownloadLocales,
    /// Download every dataset into its bundled data file.
    DownloadAll,
    /// Refresh the vendored icon and flag SVGs from their pinned revisions.
    Vendor {
        /// Fetch an upstream's pin from a local repository instead of its URL,
        /// as `NAME=PATH` (e.g. `heroicons=../heroicons`).
        #[arg(long, value_parser = parse_from)]
        from: Vec<(String, String)>,
    },
}

fn parse_from(value: &str) -> Result<(String, String)> {
    let (name, path) = value.split_once('=').context("Expected NAME=PATH")?;

    Ok((name.to_owned(), path.to_owned()))
}

fn main() -> Result<()> {
    let root = workspace_root();
    let cli = Cli::parse();

    match &cli.command {
        None => {
            // Default: bring every generated artifact up to date.
            generate_iso639(&root, cli.mapping_path.as_deref())?;
            generate_iso3166(&root)?;
            generate_locales(&root)?;
            generate_scss(&root)?;
        }
        Some(Command::Scss) => generate_scss(&root)?,
        Some(Command::Iso639) => generate_iso639(&root, cli.mapping_path.as_deref())?,
        Some(Command::Iso3166) => generate_iso3166(&root)?,
        Some(Command::Locales) => generate_locales(&root)?,
        Some(Command::DownloadLanguages) => download_languages(&root)?,
        Some(Command::DownloadCountries) => download_countries(&root)?,
        Some(Command::DownloadLocales) => download_locales(&root)?,
        Some(Command::DownloadAll) => {
            download_languages(&root)?;
            download_countries(&root)?;
            download_locales(&root)?;
        }
        Some(Command::Vendor { from }) => vendor::sync(&root, from)?,
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

/// The directory of a vendored SVG set.
fn ensure_vendored(root: &Path, dir: &str) -> Result<PathBuf> {
    let path = root.join(dir);

    if !path.is_dir() {
        bail!(
            "The vendored SVGs are missing ({}).\n\
             Run: cargo run -p tools -- vendor",
            path.display()
        );
    }

    Ok(path)
}

fn generate_scss(root: &Path) -> Result<()> {
    let flags_dir = ensure_vendored(root, FLAG_ICONS_DIR)?;
    let icons_dir = ensure_vendored(root, HEROICONS_DIR)?;

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
    let flags_dir = ensure_vendored(root, FLAG_ICONS_DIR)?;

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
    let flags_dir = ensure_vendored(root, FLAG_ICONS_DIR)?;

    let csv = read_input(root, ISO3166_CSV)?;
    write_file(
        &root.join(ISO3166_GENERATED),
        &generate::iso3166_module(&csv, &flags_dir)?,
    )?;
    Ok(())
}

fn generate_locales(root: &Path) -> Result<()> {
    let tab = read_input(root, ISO639_TAB)?;
    let csv = read_input(root, ISO3166_CSV)?;
    let json = read_input(root, LOCALES_JSON)?;

    let valid_languages = generate::language_ids(&tab)?;
    let valid_countries = generate::country_codes(&csv)?;

    write_file(
        &root.join(LOCALES_GENERATED),
        &generate::locales_module(&json, &valid_languages, &valid_countries)?,
    )?;
    Ok(())
}

fn download_languages(root: &Path) -> Result<()> {
    write_file(&root.join(ISO639_TAB), &generate::download_table()?)
}

fn download_countries(root: &Path) -> Result<()> {
    write_file(&root.join(ISO3166_CSV), &generate::download_countries()?)
}

fn download_locales(root: &Path) -> Result<()> {
    write_file(&root.join(LOCALES_JSON), &generate::download_locales()?)
}

fn read_input(root: &Path, relative: &str) -> Result<String> {
    let path = root.join(relative);
    fs::read_to_string(&path).with_context(|| anyhow!("Reading {}", path.display()))
}

fn write_file(output: &Path, body: &str) -> Result<()> {
    write_bytes(output, body.as_bytes())
}

fn write_bytes(output: &Path, body: &[u8]) -> Result<()> {
    // Skip rewriting unchanged content so we don't bump the mtime (which would retrigger
    // Trunk's file watcher into a rebuild loop) or create needless git churn.
    if fs::read(output).is_ok_and(|existing| existing == body) {
        return Ok(());
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(output, body).with_context(|| anyhow!("Writing {}", output.display()))?;
    println!("Wrote {}", output.display());
    Ok(())
}
