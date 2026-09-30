//! The SVG sets track ships a copy of, and the sync which refreshes them.
//!
//! Copies rather than submodules: a checkout has its icons, and a fresh worktree
//! builds without anybody remembering a `git submodule` command. What that costs
//! is a pin: the copy is only reproducible while [`Upstream::rev`] names the
//! revision it was taken at, so bumping one is a deliberate act and the whole of
//! a version bump.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

/// One upstream tree track takes a directory of SVGs out of. Every SVG is
/// taken: the markup names icons and flags by computed names (the country
/// picker offers every country), so a subset would be a guess with a missing
/// glyph as its only symptom.
struct Upstream {
    name: &'static str,
    url: &'static str,
    rev: &'static str,
    /// What the licence is, for the manifest. The text itself is copied out of
    /// the checkout, so it cannot drift from the revision it belongs to.
    license: &'static str,
    license_file: &'static str,
    source: &'static str,
    /// Where the SVGs land, relative to the workspace root.
    target: &'static str,
    /// Where the licence lands, relative to the workspace root.
    attribution: &'static str,
}

const HEROICONS: Upstream = Upstream {
    name: "heroicons",
    url: "https://github.com/tailwindlabs/heroicons",
    rev: "616b7a4dbbf3d011760af8066262cd5c6b3868f3",
    license: "MIT",
    license_file: "LICENSE",
    source: "optimized/24/solid",
    target: crate::HEROICONS_DIR,
    attribution: "3rdparty/heroicons",
};

const FLAG_ICONS: Upstream = Upstream {
    name: "flag-icons",
    url: "https://github.com/lipis/flag-icons",
    rev: "086f7e97d657358203916dbe84f61c2bccaa81eb",
    license: "MIT",
    license_file: "LICENSE",
    source: "flags/4x3",
    target: crate::FLAG_ICONS_DIR,
    attribution: "3rdparty/flag-icons",
};

const ALL: &[Upstream] = &[HEROICONS, FLAG_ICONS];

/// Where the manifest lands. `3rdparty/` held the submodules and now holds what
/// is owed for them: a licence per project, and this beside them.
const NOTICE: &str = "3rdparty/NOTICE";

/// Bring every vendored set back into step with its pin. `from` names a local
/// repository to fetch an upstream's pin from instead of its URL, by upstream
/// name, so a copy can be refreshed without the network.
///
/// Nothing is written where the bytes already match and every SVG upstream no
/// longer has is deleted, so a second run against the same pins leaves no diff.
pub(crate) fn sync(root: &Path, from: &[(String, String)]) -> Result<()> {
    for (name, _) in from {
        if !ALL.iter().any(|upstream| upstream.name == name) {
            bail!("No vendored upstream is called {name}");
        }
    }

    for upstream in ALL {
        let origin = match from.iter().find(|(name, _)| name == upstream.name) {
            Some((_, path)) => root
                .join(path)
                .to_str()
                .context("A non-UTF-8 repository path")?
                .to_owned(),
            None => upstream.url.to_owned(),
        };

        let scratch =
            tempfile::tempdir().with_context(|| anyhow!("Scratch space for {}", upstream.name))?;

        checkout(&origin, upstream.rev, scratch.path())
            .with_context(|| anyhow!("Fetching {} at {}", upstream.name, upstream.rev))?;

        copy_svgs(
            &scratch.path().join(upstream.source),
            &root.join(upstream.target),
        )
        .with_context(|| anyhow!("Vendoring {}", upstream.name))?;

        let license = scratch.path().join(upstream.license_file);

        let body = fs::read(&license)
            .with_context(|| anyhow!("Reading the licence at {}", license.display()))?;

        crate::write_bytes(
            &root.join(upstream.attribution).join(upstream.license_file),
            &body,
        )?;
    }

    crate::write_file(&root.join(NOTICE), &notice())
}

/// Put `rev` of `origin` in `at`. A shallow fetch of the one commit rather than
/// a clone: the history is of no interest and the revision is already decided.
fn checkout(origin: &str, rev: &str, at: &Path) -> Result<()> {
    run(at, &["init", "-q"])?;
    run(at, &["fetch", "-q", "--depth", "1", origin, rev])?;
    run(at, &["checkout", "-q", "FETCH_HEAD"])
}

/// A failure here says which command and where, since the interesting failures
/// are a missing revision and no network and they read alike otherwise.
fn run(at: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .current_dir(at)
        .args(args)
        .status()
        .with_context(|| anyhow!("Running git {}", args.join(" ")))?;

    if !status.success() {
        bail!(
            "git {} failed in {} ({status})",
            args.join(" "),
            at.display()
        );
    }

    Ok(())
}

/// Make `target` hold exactly the `.svg` files `source` offers.
fn copy_svgs(source: &Path, target: &Path) -> Result<()> {
    if !source.is_dir() {
        bail!("The upstream directory is missing: {}", source.display());
    }

    let mut vendored = BTreeSet::new();

    for entry in fs::read_dir(source).with_context(|| anyhow!("Reading {}", source.display()))? {
        let entry = entry?;
        let name = entry.file_name();

        let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(".svg")) else {
            continue;
        };

        let body = fs::read(entry.path())
            .with_context(|| anyhow!("Reading {}", entry.path().display()))?;

        crate::write_bytes(&target.join(&name), &body)?;
        vendored.insert(stem.to_owned());
    }

    if vendored.is_empty() {
        bail!("Nothing was vendored out of {}", source.display());
    }

    for entry in fs::read_dir(target).with_context(|| anyhow!("Reading {}", target.display()))? {
        let entry = entry?;
        let name = entry.file_name();

        let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(".svg")) else {
            continue;
        };

        if !vendored.contains(stem) {
            fs::remove_file(entry.path())
                .with_context(|| anyhow!("Removing {}", entry.path().display()))?;
            println!("Removed {}", entry.path().display());
        }
    }

    Ok(())
}

/// The attribution manifest. Generated from the same table the sync fetches
/// from, so a bumped pin cannot leave the attribution standing at the old one.
fn notice() -> String {
    use std::fmt::Write;

    let mut o = String::new();

    _ = writeln!(
        o,
        "track ships a copy of the SVG sets below. Each is taken at the revision"
    );
    _ = writeln!(
        o,
        "named, and refreshed by `cargo run -p tools -- vendor`; this file is written"
    );
    _ = writeln!(o, "by that command and not by hand.");

    for upstream in ALL {
        _ = writeln!(o);
        _ = writeln!(o, "{}", upstream.name);
        _ = writeln!(o, "    Upstream   {}", upstream.url);
        _ = writeln!(o, "    Revision   {}", upstream.rev);
        _ = writeln!(
            o,
            "    Taken      {} -> {}",
            upstream.source, upstream.target
        );
        _ = writeln!(
            o,
            "    Licence    {}, in {}/{}",
            upstream.license, upstream.attribution, upstream.license_file
        );
    }

    o
}
