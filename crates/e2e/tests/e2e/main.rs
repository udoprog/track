//! Browser tests for track. Every test gets a server of its own, with a fresh
//! database and image cache in the run's sandbox, serving the frontend that
//! yew-e2e builds for the run. A fresh database has no API keys, so nothing is
//! synced from the remotes.
//!
//! ```text
//! cargo test -p e2e
//! cargo test -p e2e -- settings::
//! cargo test -p e2e -- --headed --last-session
//! ```

mod navigation;
mod settings;

use std::fs::File;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::OnceCell;
use yew_e2e::{Config, Fixture};

static SANDBOX: OnceLock<PathBuf> = OnceLock::new();
static SERVER: OnceCell<PathBuf> = OnceCell::const_new();

/// How long a freshly started server may take to start listening.
const LISTEN_TIMEOUT: Duration = Duration::from_secs(10);

/// A track server for one test.
struct Track {
    port: u16,
    child: Child,
    // Dropped after the server is killed, taking its database with it.
    _dir: TempDir,
}

impl Fixture for Track {
    type Setup = ();

    fn config() -> Config {
        Config::default().about("Browser tests for track.")
    }

    fn enter_sandbox(root: &Path) -> Result<()> {
        _ = SANDBOX.set(root.to_owned());
        Ok(())
    }

    async fn start((): ()) -> Result<Self> {
        let server = SERVER.get_or_try_init(build_server).await?;
        let sandbox = SANDBOX.get().context("the sandbox was not entered")?;
        let dir = TempDir::new_in(sandbox)?;

        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?
            .local_addr()?
            .port();

        let log = File::create(dir.path().join("track.log"))?;

        let mut child = Command::new(server)
            .arg("--db")
            .arg(dir.path().join("track.db"))
            .arg("--cache-dir")
            .arg(dir.path().join("image-cache"))
            .arg("--bind")
            .arg(format!("127.0.0.1:{port}"))
            .arg("--dist")
            .arg(yew_e2e::dist()?)
            .stdout(log.try_clone()?)
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
            .context("starting track")?;

        let deadline = tokio::time::Instant::now() + LISTEN_TIMEOUT;

        while tokio::time::Instant::now() < deadline {
            if let Some(status) = child.try_wait()? {
                let log = std::fs::read_to_string(dir.path().join("track.log"))?;
                bail!("track exited with {status}:\n{log}");
            }

            if TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .await
                .is_ok()
            {
                return Ok(Self {
                    port,
                    child,
                    _dir: dir,
                });
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        bail!("track did not listen on port {port} within {LISTEN_TIMEOUT:?}")
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    async fn quit(mut self) -> Result<()> {
        self.child.kill().await?;
        Ok(())
    }
}

/// Build the server binary, once per run: cargo only builds this suite's own
/// dependencies before running it.
async fn build_server() -> Result<PathBuf> {
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "track", "--bin", "track"])
        .status()
        .await
        .context("running cargo build")?;

    ensure!(status.success(), "building track failed: {status}");
    Ok(yew_e2e::target_dir()?.join("debug").join("track"))
}

yew_e2e::harness! {
    Track;
    navigation::{opens_every_page, toolbar_icons_are_small},
    settings::{theme_applies_live, theme_is_remembered},
}
