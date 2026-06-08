use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let tab_path = manifest_dir.join("data").join("iso-639-3.tab");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", tab_path.display());

    let tab = fs::read_to_string(&tab_path)?;
    let generated = update_languages::generate_module_from_tab(&tab)?;
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let target = out_dir.join("generated.rs");
    fs::write(target, generated)?;
    Ok(())
}
