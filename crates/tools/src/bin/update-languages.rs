use std::env;
use std::fs;

use anyhow::{Context, Result, anyhow, bail};

fn main() -> Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let data_only = if args.first().is_some_and(|arg| arg == "--data-module") {
        args.remove(0);
        true
    } else {
        false
    };

    if args.len() > 1 {
        bail!("Usage: update-languages [--data-module] [output-file]");
    }

    let out = args.pop();
    let module = if data_only {
        update_languages::generate_data_module()?
    } else {
        update_languages::generate_standalone_module()?
    };

    match out {
        Some(path) => {
            fs::write(&path, module)
                .with_context(|| anyhow!("Writing generated output to {path}"))?;
            eprintln!("Wrote language sets to {path}");
        }
        None => {
            print!("{module}");
        }
    }

    Ok(())
}
