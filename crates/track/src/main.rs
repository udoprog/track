use core::time::Duration;
use std::process::ExitCode;
use std::time::Instant;

use anyhow::Result;
use tokio::runtime::Builder;

const TEN: Duration = Duration::from_secs(10);

pub fn main() -> Result<ExitCode> {
    let runtime = Builder::new_multi_thread().enable_all().build()?;

    let result = runtime.block_on(track::server());

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
