use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

#[path = "manual.rs"]
mod manual;

include!("style.rs");

fn main() -> Result<(), Box<dyn Error>> {
    style().compile();

    let help = Path::new("..").join(manual::HELP);
    let icons = Path::new("../crates/frontend/icons");
    println!("cargo::rerun-if-changed={}", help.display());

    // help.rs lists the files in name order (its tests check it), so the
    // sorted directory is the app's order.
    let mut files = Vec::new();

    for entry in fs::read_dir(&help)? {
        let path = entry?.path();

        if path.extension().is_some_and(|e| e == "md") {
            println!("cargo::rerun-if-changed={}", path.display());
            let name = path
                .file_name()
                .ok_or("no file name")?
                .to_string_lossy()
                .into_owned();
            files.push((name, fs::read_to_string(&path)?));
        }
    }

    files.sort();

    let mut sections = Vec::new();

    for (name, source) in &files {
        let section = manual::parse(source).ok_or_else(|| format!("{name}: no id/title header"))?;
        sections.push((name, section));
    }

    let ids = sections
        .iter()
        .map(|(_, s)| s.id.clone())
        .collect::<BTreeSet<_>>();

    let icon = |name: &str| {
        let path = icons.join(format!("{name}.svg"));
        println!("cargo::rerun-if-changed={}", path.display());
        fs::read_to_string(path).ok()
    };

    let samples = manual::Samples { icon: &icon };

    let mut out = String::new();
    writeln!(out, "pub static PAGES: &[Page] = &[")?;

    for (name, section) in &sections {
        let page = manual::render(&section.body, &ids, &samples)
            .map_err(|error| format!("{name}: {error}"))?;

        writeln!(
            out,
            "    Page {{ id: {:?}, file: {name:?}, title: {:?}, html: {:?}, headings: &[",
            section.id, section.title, page.html
        )?;

        for (anchor, text) in &page.headings {
            writeln!(out, "        ({anchor:?}, {text:?}),")?;
        }

        writeln!(out, "    ] }},")?;
    }

    writeln!(out, "];")?;

    let out_dir = std::env::var("OUT_DIR")?;
    fs::write(Path::new(&out_dir).join("manual.rs"), out)?;
    Ok(())
}
