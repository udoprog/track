use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);

    let csv_path = manifest_dir.join("data").join("all.csv");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", csv_path.display());

    let mut flags_dir = manifest_dir.clone();
    flags_dir.push("..");
    flags_dir.push("..");
    flags_dir.push("3rdparty");
    flags_dir.push("flag-icons");
    flags_dir.push("flags");
    flags_dir.push("4x3");

    if !flags_dir.is_dir() {
        panic!(
            "Expected flag icons to be present at {}, but the directory does not exist",
            flags_dir.display()
        );
    }

    let csv = fs::read_to_string(&csv_path)?;
    let generated = update_languages::generate_countries_module(&csv, &flags_dir)?;

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(out_dir.join("generated_countries.rs"), generated)?;

    Ok(())
}
