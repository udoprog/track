use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};

mod generate;
mod vendor;

// Vendored SVG sets; see `vendor.rs`.
const FLAG_ICONS_DIR: &str = "crates/frontend/flags";
const HEROICONS_DIR: &str = "crates/frontend/icons";

// Generated, committed name lists.
const FLAGS_SCSS: &str = "crates/frontend/style/_flags_generated.scss";
const ICONS_SCSS: &str = "crates/frontend/style/_icons_generated.scss";
const FLAGS_RS: &str = "crates/frontend/src/flags.rs";

/// Refresh the files generated from the vendored SVGs. With no subcommand it regenerates
/// them from the committed SVGs.
#[derive(Parser)]
#[command(about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Regenerate the flag/icon name lists (the trug pre_build hook).
    Scss,
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
        None | Some(Command::Scss) => generate_scss(&root)?,
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

    write_file(&root.join(FLAGS_RS), &generate::flags_module(&flags_dir)?)?;

    Ok(())
}

fn write_file(output: &Path, body: &str) -> Result<()> {
    write_bytes(output, body.as_bytes())
}

fn write_bytes(output: &Path, body: &[u8]) -> Result<()> {
    // Skip rewriting unchanged content so we don't bump the mtime (which would retrigger
    // trug's file watcher into a rebuild loop) or create needless git churn.
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
