use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let tab_path = manifest_dir.join("data").join("iso-639-3.tab");
    let to_3166_1_path = manifest_dir.join("data").join("to-3166-1.txt");
    let countries_path = manifest_dir.join("data").join("countries.txt");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", tab_path.display());
    println!("cargo:rerun-if-changed={}", to_3166_1_path.display());
    println!("cargo:rerun-if-changed={}", countries_path.display());

    let tab = fs::read_to_string(&tab_path)?;
    let generated = update_languages::generate_module_from_tab(&tab)?;
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let target = out_dir.join("generated.rs");
    fs::write(target, generated)?;

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("..");
    path.push("3rdparty");
    path.push("flag-icons");
    path.push("flags");
    path.push("4x3");

    if !path.is_dir() {
        panic!(
            "expected flag icons to be present at {}, but the directory does not exist",
            path.display()
        );
    }

    let to_3166_1 = fs::read_to_string(&to_3166_1_path)?;
    let generated = update_languages::generate_module_from_to_3166_1(&to_3166_1, &path)?;
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let target = out_dir.join("generated_to_3166_1.rs");
    fs::write(target, generated)?;

    let countries = fs::read_to_string(&countries_path)?;
    let generated = update_languages::generate_module_from_countries(&countries)?;
    let target = out_dir.join("generated_countries.rs");
    fs::write(target, generated)?;

    Ok(())
}
