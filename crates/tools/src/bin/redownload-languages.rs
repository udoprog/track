use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let output = match args.next() {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("crates/iso639/data/iso-639-3.tab"),
    };

    if args.next().is_some() {
        bail!("usage: redownload-languages [output-file]");
    }

    let body = update_languages::download_table()?;

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&output, body)?;
    eprintln!("wrote {}", output.display());
    Ok(())
}
