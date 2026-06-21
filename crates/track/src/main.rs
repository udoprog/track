use core::time::Duration;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use tokio::runtime::Builder;

const TEN: Duration = Duration::from_secs(10);

/// ontv-musli-web tracking service. With no subcommand it runs the server;
/// the subcommands back up the irreplaceable data (remotes + watched history).
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<track::BackupCommand>,

    /// Path to the SQLite database (used by the backup subcommands).
    #[arg(long, default_value = "track.db")]
    db: PathBuf,

    /// Add logging directives (used by the backup subcommands).
    #[arg(long)]
    log: Vec<String>,

    #[command(flatten)]
    args: track::Args,
}

pub fn main() -> Result<ExitCode> {
    let cli = Cli::parse();

    let runtime = Builder::new_multi_thread().enable_all().build()?;

    let Some(command) = cli.command else {
        return run_server(runtime, cli.args, &cli.db, &cli.log);
    };

    runtime.block_on(track::backup(&cli.db, &cli.log, command))?;
    Ok(ExitCode::SUCCESS)
}

fn run_server(
    runtime: tokio::runtime::Runtime,
    args: track::Args,
    db: &Path,
    log: &[String],
) -> Result<ExitCode> {
    let result = runtime.block_on(track::server(args, db, log));

    let start = Instant::now();

    runtime.shutdown_timeout(TEN);

    let duration = Instant::now().duration_since(start);

    if ((duration.as_millis() as i64) - (TEN.as_millis() as i64)).abs() < 100 {
        tracing::warn!(
            "Server shutdown timed out and background tasks were killed, you likely have some long-running tasks that leaked"
        );
    }

    result
}
