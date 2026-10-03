//! A self-hosted web service for tracking show and movie progress and what to
//! watch next.
//!
//! This is a reimagining of [ontv], which in turn was a reimagining of my old
//! Python-based CLI application. It now runs as a server backed by SQLite with
//! a web frontend written in [Yew].
//!
//! Still in the experimental stage. Users beware!
//!
//! [ontv]: https://github.com/udoprog/ontv
//! [Yew]: https://yew.rs
//!
//! <br>
//!
//! ## Features
//!
//! * A dashboard of what to watch next, upcoming releases, and a weekly
//!   schedule.
//! * A queue of what's pending to be watched.
//! * Detailed watch history for shows and movies.
//! * Show, movie, and people pages with translated titles and overviews.
//! * Metadata synced from [TheTVDB], [TheMovieDB], and [TVmaze], with
//!   configurable sync sources, languages, and release filters.
//! * Search for new shows and movies to track.
//! * Backups of your remotes and watch history as JSON lines.
//!
//! [TheTVDB]: https://thetvdb.com
//! [TheMovieDB]: https://www.themoviedb.org
//! [TVmaze]: https://www.tvmaze.com
//!
//! <br>
//!
//! ## Running track
//!
//! The frontend is built with [trunk] into `dist/`. The server embeds that
//! directory, so it has to be built first:
//!
//! ```text
//! $ trunk build --release
//! $ cargo run --release
//! ```
//!
//! By default the server listens on `127.0.0.1:3000`, stores its state in
//! `track.db`, and caches images in `image-cache`. See `--help` for how to
//! change this.
//!
//! Once it is running, go to `Settings` and configure your API keys for
//! TheTVDB and TheMovieDB. Unfortunately I cannot help you with this.
//!
//! [trunk]: https://trunkrs.dev
//!
//! <br>
//!
//! ## Development
//!
//! During development you can run the server and the frontend separately. The
//! trunk dev server proxies `/api/` and `/ws` to the server on port `3000`:
//!
//! ```text
//! $ cargo run
//! $ trunk serve
//! ```
//!
//! To preview loading states as they would look on a slow connection, you can
//! inject an artificial delay in milliseconds into every websocket request:
//!
//! ```text
//! $ cargo run -- --delay 200..800
//! ```
//!
//! The browser tests in `crates/e2e` use [yew-e2e]. They build the frontend and
//! the server, and give every test its own server with a fresh database, so they
//! never touch `track.db`. They need Firefox with `geckodriver`, or Chrome:
//!
//! ```text
//! $ cargo test -p e2e
//! $ cargo test -p e2e -- settings::
//! $ cargo test -p e2e -- --headed --last-session
//! ```
//!
//! [yew-e2e]: https://github.com/udoprog/yew-e2e
//!
//! <br>
//!
//! ## Importing from ontv
//!
//! If you have been using [ontv], its YAML database can be imported like this:
//!
//! ```text
//! $ cargo run --bin import -- --source ~/.config/ontv --db track.db
//! ```
//!
//! Run it once, into a fresh database; it is not safe to rerun. Shows and movies
//! are only matched by remote id, so a second run can fail or duplicate entries,
//! and the settings from `config.yaml` (API keys, theme, dashboard page) replace
//! the current configuration and preferences on every run. If an import is
//! aborted, start over with a fresh database.
//!
//! This will take a while, so go get a ☕.
//!
//! <br>
//!
//! ## Backing up your data
//!
//! Most of the data in the database can be recovered by syncing it again from
//! the remotes. What can't be recovered is the users, the remotes of each show and
//! movie, what each user tracks, their preferences and their watch history, so
//! those are what the backup covers. Passwords, sessions and login links are not
//! exported, so a restored user needs a new login link to sign in.
//!
//! ```text
//! $ track export --output backup.jsonl
//! $ track import --input backup.jsonl
//! ```
//!
//! A backup is newline-delimited JSON. Lines starting with `#` are comments, so
//! you can annotate a backup by hand. Importing is idempotent, so entries which
//! already exist are skipped.
//!
//! Export only reads: it refuses a database that does not exist or has pending
//! migrations, and reads everything from one snapshot, so it can run while the
//! server is up. Stop the server before importing: import writes without
//! journaling, so it must not run alongside the server and is not crash-safe.

mod app_broadcast;
mod background;
mod backup;
mod cache;
mod db;
mod entry;
mod http;
mod identity;
#[cfg(feature = "import")]
mod import;
mod login_throttle;
mod pending;
mod proxy;
mod remote;
mod shutdown;
#[cfg(feature = "bundle")]
mod static_assets;
mod sync;
mod task_queue;
#[cfg(test)]
mod tests;
mod tmdb;
mod tvdb;
mod tvmaze;
mod web;
mod ws;

pub use self::backup::{BackupCommand, backup};
pub use self::entry::{Args, serve, server};
#[cfg(feature = "import")]
pub use self::import::import;
