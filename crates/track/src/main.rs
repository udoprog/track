use std::process::ExitCode;

use anyhow::Result;

#[tokio::main]
pub async fn main() -> Result<ExitCode> {
    track::server().await
}
