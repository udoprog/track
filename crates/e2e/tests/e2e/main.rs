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

mod dashboard;
mod media;
mod navigation;
mod people;
mod queue;
mod search;
mod settings;
mod show;

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

/// Media for the tests that ask for it with `(seeded)`.
const SEED: &str = include_str!("seed.sql");
/// More shows on top of [`SEED`] for the tests that ask with `(crowded)`.
const CROWDED: &str = include_str!("crowded.sql");

/// What a test asks of its server.
#[derive(Default)]
struct Setup {
    /// Start with the library in `seed.sql`.
    seeded: bool,
    /// Start with `seed.sql` and the shows in `crowded.sql`.
    crowded: bool,
}

/// A track server for one test.
struct Track {
    port: u16,
    child: Child,
    // Dropped after the server is killed, taking its database with it.
    _dir: TempDir,
}

impl Fixture for Track {
    type Setup = Setup;

    fn config() -> Config {
        Config::default().about("Browser tests for track.")
    }

    fn enter_sandbox(root: &Path) -> Result<()> {
        _ = SANDBOX.set(root.to_owned());
        Ok(())
    }

    async fn start(setup: Setup) -> Result<Self> {
        let server = SERVER.get_or_try_init(build_server).await?;
        let sandbox = SANDBOX.get().context("the sandbox was not entered")?;
        let dir = TempDir::new_in(sandbox)?;

        if setup.seeded || setup.crowded {
            // The server creates the schema; the seed goes in while it is down.
            let (mut child, _) = spawn(server, dir.path()).await?;
            child.kill().await?;

            let c = sqll::Connection::open(dir.path().join("track.db"))?;
            c.execute(SEED).context("seeding the database")?;

            if setup.crowded {
                c.execute(CROWDED).context("crowding the database")?;
            }
        }

        let (child, port) = spawn(server, dir.path()).await?;

        Ok(Self {
            port,
            child,
            _dir: dir,
        })
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    async fn quit(mut self) -> Result<()> {
        self.child.kill().await?;
        Ok(())
    }
}

/// Start a server on a free port against the database in `dir`, and wait for
/// it to listen.
async fn spawn(server: &Path, dir: &Path) -> Result<(Child, u16)> {
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?
        .local_addr()?
        .port();

    let log = File::create(dir.join("track.log"))?;

    let mut child = Command::new(server)
        .arg("--db")
        .arg(dir.join("track.db"))
        .arg("--cache-dir")
        .arg(dir.join("image-cache"))
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
            let log = std::fs::read_to_string(dir.join("track.log"))?;
            bail!("track exited with {status}:\n{log}");
        }

        if TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_ok()
        {
            return Ok((child, port));
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    bail!("track did not listen on port {port} within {LISTEN_TIMEOUT:?}")
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
    dashboard::{fills_rows_with_relative_dates(seeded), labels_its_filters, mobile_cards_always_have_a_picture(seeded), schedule_names_its_days, upcoming_days_keep_their_width, marks_watched_in_one_click(seeded), marks_watched_at_a_chosen_time(seeded)},
    media::{shows_a_poster_grid(seeded)},
    navigation::{opens_every_page, page_scrolls_the_window, toolbar_icons_are_small, every_button_has_a_title(seeded)},
    people::{lists_people_by_name(seeded)},
    queue::{lists_tasks_in_columns, keeps_rows_in_place(seeded), follows_the_next_task(crowded), shows_failed_tasks(seeded)},
    search::{focuses_the_input},
    settings::{reorders_sync_sources, theme_applies_live, theme_is_remembered},
    show::{episode_menu_holds_the_other_actions(seeded), has_a_heading(seeded), phones_have_no_episode_rail(seeded), seasons_count_watched_episodes(seeded), watched_episodes_are_compact(seeded)},
}
