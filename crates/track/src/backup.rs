//! Backup of the irreplaceable data in the service: the remotes, the users and
//! each user's tracking, watched history and preferences. Everything else
//! (titles, episodes, images, release dates) is reproducible by re-syncing from
//! the remotes, so it is deliberately not exported. Neither are password hashes,
//! sessions, login links or system config; imported users sign in through a new
//! login link. The watch-next queue is rebuilt on import rather than exported.
//!
//! The format is newline-delimited JSON: one [`BackupRow`] per line, tagged by
//! a `type` field. Lines whose first non-whitespace character is `#` are
//! comments and blank lines are ignored, so a backup file can be annotated by
//! hand. Import is idempotent entries that already exist are explicitly counted
//! and skipped.
//!
//! Identifiers are exported as their canonical string form and timestamps as UTC
//! ISO-8601 strings. Shows and movies referenced by tracking or preferences
//! are created as placeholders for sync to fill in; remotes and watched entries
//! are not foreign keys, so they never create one. Rows name their user by
//! login; users come first in a backup so import can create them, and watched
//! entries from before there were users belong to root.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use tracing::Level;

use crate::db::users::Conflict;
use crate::db::{Database, OpenMode};

/// (De)serialize a value via its `Display`/`FromStr` string form rather than its
/// default representation. Used so opaque ids export as their canonical strings.
mod as_string {
    use std::fmt::Display;
    use std::str::FromStr;

    use serde::{Deserialize as _, Deserializer, Serializer, de::Error};

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(Error::custom)
    }
}

/// (De)serialize a [`api::Timestamp`] as a UTC ISO-8601 string (with a `Z` zone).
mod ts_utc {
    use serde::{Deserialize as _, Deserializer, Serializer, de::Error};

    pub fn serialize<S>(ts: &api::Timestamp, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Force millisecond precision (jiff honors the precision flag), so the
        // exported string always carries the `.mmm` the database stores.
        serializer.collect_str(&format_args!("{:.3}", ts.inner()))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<api::Timestamp, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let ts: jiff::Timestamp = s.parse().map_err(Error::custom)?;
        Ok(api::Timestamp::from_jiff(ts))
    }
}

/// One exported row. The `type` tag lets the file be read back as this enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BackupRow {
    /// A remote attached to a show, referenced by its identifier and exported as
    /// its structure (`source` + `value`).
    ShowRemote {
        #[serde(with = "as_string")]
        id: api::RemoteId,
        #[serde(with = "as_string")]
        show: api::ShowId,
        source: api::RemoteSource,
        value: api::RemoteValue,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slug: Option<String>,
        enabled: bool,
        priority: i32,
        /// The sync kinds this remote contributes; absent means "inherit the default".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sync_kinds: Option<api::SyncKindSet>,
    },
    /// A remote attached to a movie.
    MovieRemote {
        #[serde(with = "as_string")]
        id: api::RemoteId,
        #[serde(with = "as_string")]
        movie: api::MovieId,
        source: api::RemoteSource,
        value: api::RemoteValue,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slug: Option<String>,
        enabled: bool,
        priority: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sync_kinds: Option<api::SyncKindSet>,
    },
    /// A user account, without credentials.
    User {
        login: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
        #[serde(with = "as_string")]
        role: auth::UserRole,
    },
    /// A show a user tracks.
    TrackedShow {
        user: String,
        #[serde(with = "as_string")]
        show: api::ShowId,
    },
    /// A movie a user tracks.
    TrackedMovie {
        user: String,
        #[serde(with = "as_string")]
        movie: api::MovieId,
    },
    /// One of a user's own preferences.
    UserPreference {
        user: String,
        #[serde(with = "as_string")]
        key: api::PreferenceKey,
        value: serde_json::Value,
    },
    /// A user's preference for one show.
    ShowPreference {
        user: String,
        #[serde(with = "as_string")]
        show: api::ShowId,
        #[serde(with = "as_string")]
        key: api::PreferenceKey,
        value: serde_json::Value,
    },
    /// A user's preference for one movie.
    MoviePreference {
        user: String,
        #[serde(with = "as_string")]
        movie: api::MovieId,
        #[serde(with = "as_string")]
        key: api::PreferenceKey,
        value: serde_json::Value,
    },
    /// A single watched episode, keyed by show + season + episode (not episode id,
    /// so it survives a re-sync).
    WatchedEpisode {
        #[serde(with = "as_string")]
        id: api::WatchedId,
        /// The owner's login; absent in backups from before there were users.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user: Option<String>,
        #[serde(with = "as_string")]
        show: api::ShowId,
        season: api::SeasonNumber,
        episode: u32,
        #[serde(with = "ts_utc")]
        timestamp: api::Timestamp,
    },
    /// A single watched movie.
    WatchedMovie {
        #[serde(with = "as_string")]
        id: api::WatchedId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user: Option<String>,
        #[serde(with = "as_string")]
        movie: api::MovieId,
        #[serde(with = "ts_utc")]
        timestamp: api::Timestamp,
    },
}

/// Tally of what an import did, reported per category.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ImportStats {
    inserted: u64,
    ignored: u64,
}

impl ImportStats {
    fn record(&mut self, inserted: bool) {
        if inserted {
            self.inserted += 1;
        } else {
            self.ignored += 1;
        }
    }
}

/// A stored preference key, or `None` (with a warning) for one this version
/// does not know, which is left out of the backup.
fn preference_key(user: &str, key: &str) -> Option<api::PreferenceKey> {
    match key.parse() {
        Ok(key) => Some(key),
        Err(error) => {
            tracing::warn!("Skipping preference {key:?} of {user}: {error}");
            None
        }
    }
}

/// Write every user, remote, tracking, preference and watched entry as a JSON
/// line to `out`. Users come first so import can create them before their rows.
async fn export(db: &Database, mut out: impl Write) -> Result<()> {
    let mut write = |row: BackupRow| -> Result<()> {
        writeln!(out, "{}", serde_json::to_string(&row)?)?;
        Ok(())
    };

    let snapshot = db.export_snapshot().await?;

    for user in snapshot.users {
        write(BackupRow::User {
            login: user.login,
            email: user.email,
            role: user.role,
        })?;
    }

    for r in snapshot.show_remotes {
        let row = BackupRow::ShowRemote {
            id: r.id,
            show: r.owner,
            source: r.source,
            value: r.value,
            slug: r.slug,
            enabled: r.enabled,
            priority: r.priority,
            sync_kinds: r.sync_kinds,
        };
        write(row)?;
    }

    for r in snapshot.movie_remotes {
        let row = BackupRow::MovieRemote {
            id: r.id,
            movie: r.owner,
            source: r.source,
            value: r.value,
            slug: r.slug,
            enabled: r.enabled,
            priority: r.priority,
            sync_kinds: r.sync_kinds,
        };
        write(row)?;
    }

    for (user, show) in snapshot.user_data.tracked_shows {
        write(BackupRow::TrackedShow { user, show })?;
    }

    for (user, movie) in snapshot.user_data.tracked_movies {
        write(BackupRow::TrackedMovie { user, movie })?;
    }

    for (user, key, value) in snapshot.user_data.user_config {
        if let Some(key) = preference_key(&user, &key) {
            let value = serde_json::from_str(&value)?;
            write(BackupRow::UserPreference { user, key, value })?;
        }
    }

    for (user, show, key, value) in snapshot.user_data.show_config {
        if let Some(key) = preference_key(&user, &key) {
            let value = serde_json::from_str(&value)?;
            write(BackupRow::ShowPreference {
                user,
                show,
                key,
                value,
            })?;
        }
    }

    for (user, movie, key, value) in snapshot.user_data.movie_config {
        if let Some(key) = preference_key(&user, &key) {
            let value = serde_json::from_str(&value)?;
            write(BackupRow::MoviePreference {
                user,
                movie,
                key,
                value,
            })?;
        }
    }

    for (id, user, timestamp, show, season, episode) in snapshot.watched_episodes {
        let row = BackupRow::WatchedEpisode {
            id,
            user: Some(user),
            show,
            season,
            episode,
            timestamp,
        };
        write(row)?;
    }

    for (id, user, timestamp, movie) in snapshot.watched_movies {
        let row = BackupRow::WatchedMovie {
            id,
            user: Some(user),
            movie,
            timestamp,
        };
        write(row)?;
    }

    out.flush()?;
    Ok(())
}

/// Whether a line is a comment (first non-whitespace char is `#`) or blank.
fn is_skippable(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Totals for each category an import touched.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ImportReport {
    users: ImportStats,
    remotes: ImportStats,
    tracked: ImportStats,
    preferences: ImportStats,
    watched: ImportStats,
}

/// Resolves the users named by rows, once per login.
struct Users<'a> {
    db: &'a Database,
    by_login: HashMap<String, api::UserId>,
    default: Option<api::UserId>,
}

impl<'a> Users<'a> {
    fn new(db: &'a Database) -> Self {
        Self {
            db,
            by_login: HashMap::new(),
            default: None,
        }
    }

    /// Create a user without a password unless the login is taken. Returns
    /// whether it was created.
    async fn create(
        &mut self,
        login: String,
        email: Option<String>,
        role: auth::UserRole,
    ) -> Result<bool> {
        if let Some(user) = self.db.user_by_login(&login).await? {
            self.by_login.insert(login, user.id);
            return Ok(false);
        }

        let user = self
            .db
            .create_user(&login, email.as_deref(), role, api::Timestamp::now())
            .await?
            .map_err(|conflict| {
                let field = match conflict {
                    Conflict::Login => "login",
                    Conflict::Email => "email",
                };

                anyhow!("Cannot create the user {login:?}: its {field} is taken")
            })?;

        self.by_login.insert(login, user.id);
        Ok(true)
    }

    async fn get(&mut self, login: Option<String>) -> Result<api::UserId> {
        let Some(login) = login else {
            if let Some(id) = self.default {
                return Ok(id);
            }

            let id = self.db.default_owner().await?;
            self.default = Some(id);
            return Ok(id);
        };

        if let Some(&id) = self.by_login.get(&login) {
            return Ok(id);
        }

        let user = self
            .db
            .user_by_login(&login)
            .await?
            .with_context(|| anyhow!("No user with the login {login:?}; create it first"))?;

        self.by_login.insert(login, user.id);
        Ok(user.id)
    }
}

/// A preference key checked against the scope it is imported into, with its
/// value as stored JSON text.
fn preference(
    key: api::PreferenceKey,
    scope: api::PreferenceScope,
    value: serde_json::Value,
) -> Result<(api::PreferenceKey, String)> {
    if !key.allowed_in(scope) {
        return Err(anyhow!("The preference {key} is not allowed for {scope:?}"));
    }

    Ok((key, value.to_string()))
}

/// Read JSON lines from `input` and apply them idempotently, then rebuild the
/// watch-next queue for the imported tracking.
async fn import(db: &Database, input: impl BufRead) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let mut users = Users::new(db);
    let mut tracked_shows = Vec::new();
    let mut tracked_movies = false;

    for (n, line) in input.lines().enumerate() {
        let line = line.with_context(|| anyhow!("Reading line {}", n + 1))?;

        if is_skippable(&line) {
            continue;
        }

        let row: BackupRow = serde_json::from_str(&line)
            .with_context(|| anyhow!("Parsing line {}: {line}", n + 1))?;

        match row {
            BackupRow::User { login, email, role } => {
                let inserted = users.create(login, email, role).await?;
                report.users.record(inserted);
            }
            BackupRow::ShowRemote {
                id,
                show,
                source,
                value,
                slug,
                enabled,
                priority,
                sync_kinds,
            } => {
                let inserted = db
                    .import_show_remote(crate::db::ExportRemote {
                        id,
                        owner: show,
                        source,
                        value,
                        slug,
                        enabled,
                        priority,
                        sync_kinds,
                    })
                    .await?;
                report.remotes.record(inserted);
            }
            BackupRow::MovieRemote {
                id,
                movie,
                source,
                value,
                slug,
                enabled,
                priority,
                sync_kinds,
            } => {
                let inserted = db
                    .import_movie_remote(crate::db::ExportRemote {
                        id,
                        owner: movie,
                        source,
                        value,
                        slug,
                        enabled,
                        priority,
                        sync_kinds,
                    })
                    .await?;
                report.remotes.record(inserted);
            }
            BackupRow::TrackedShow { user, show } => {
                let user = users.get(Some(user)).await?;
                let inserted = db.import_tracked_show(user, show).await?;
                report.tracked.record(inserted);
                tracked_shows.push((user, show));
            }
            BackupRow::TrackedMovie { user, movie } => {
                let user = users.get(Some(user)).await?;
                let inserted = db.import_tracked_movie(user, movie).await?;
                report.tracked.record(inserted);
                tracked_movies = true;
            }
            BackupRow::UserPreference { user, key, value } => {
                let user = users.get(Some(user)).await?;
                let (key, value) = preference(key, api::PreferenceScope::User, value)?;
                let inserted = db.import_user_preference(user, key, value).await?;
                report.preferences.record(inserted);
            }
            BackupRow::ShowPreference {
                user,
                show,
                key,
                value,
            } => {
                let user = users.get(Some(user)).await?;
                let (key, value) = preference(key, api::PreferenceScope::Show, value)?;
                let inserted = db.import_show_preference(user, show, key, value).await?;
                report.preferences.record(inserted);
            }
            BackupRow::MoviePreference {
                user,
                movie,
                key,
                value,
            } => {
                let user = users.get(Some(user)).await?;
                let (key, value) = preference(key, api::PreferenceScope::Movie, value)?;
                let inserted = db.import_movie_preference(user, movie, key, value).await?;
                report.preferences.record(inserted);
            }
            BackupRow::WatchedEpisode {
                id,
                user,
                show,
                season,
                episode,
                timestamp,
            } => {
                let user = users.get(user).await?;
                let inserted = db
                    .import_watched_episode(user, id, timestamp, show, season, episode)
                    .await?;
                report.watched.record(inserted);
            }
            BackupRow::WatchedMovie {
                id,
                user,
                movie,
                timestamp,
            } => {
                let user = users.get(user).await?;
                let inserted = db.import_watched_movie(user, id, timestamp, movie).await?;
                report.watched.record(inserted);
            }
        }
    }

    // Shows without episodes yet (placeholders) get their queue after sync.
    let now = api::Timestamp::now();

    for (user, show) in tracked_shows {
        db.fill_pending_for_user_show(user, show, now).await?;
    }

    if tracked_movies {
        crate::background::discover_pending_movies(db).await?;
    }

    Ok(report)
}

/// Subcommands of the `track` binary that back up the irreplaceable data.
#[derive(Subcommand)]
pub enum BackupCommand {
    /// Write users, remotes, tracking, preferences and watched history as JSON lines.
    Export {
        /// Output file; defaults to stdout.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Apply a previously exported backup, idempotently.
    Import {
        /// Input file; defaults to stdin.
        #[arg(long)]
        input: Option<PathBuf>,
    },
}

/// Run a backup subcommand against the database at `db`.
pub async fn backup(db: &Path, log: &[String], command: BackupCommand) -> Result<()> {
    let mut filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();

    for directive in log {
        filter = filter.add_directive(directive.parse()?);
    }

    // Logs go to stderr: an export without --output writes the backup to stdout.
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    match command {
        BackupCommand::Export { output } => {
            let database = Database::open(db, OpenMode::ReadOnly, 1)
                .with_context(|| anyhow!("Opening database at {}", db.display()))?;

            match output {
                Some(path) => {
                    let file = std::fs::File::create(&path)
                        .with_context(|| anyhow!("Creating {}", path.display()))?;
                    export(&database, std::io::BufWriter::new(file)).await?;
                    tracing::info!("Wrote backup to {}", path.display());
                }
                None => export(&database, std::io::stdout().lock()).await?,
            }
        }
        BackupCommand::Import { input } => {
            let database = Database::open(db, OpenMode::Bulk, 1)
                .with_context(|| anyhow!("Opening database at {}", db.display()))?;

            let report = match input {
                Some(path) => {
                    let file = std::fs::File::open(&path)
                        .with_context(|| anyhow!("Opening {}", path.display()))?;
                    import(&database, std::io::BufReader::new(file)).await?
                }
                None => import(&database, std::io::stdin().lock()).await?,
            };

            for (name, stats) in [
                ("Users", report.users),
                ("Remotes", report.remotes),
                ("Tracked", report.tracked),
                ("Preferences", report.preferences),
                ("Watched", report.watched),
            ] {
                tracing::info!(
                    "{name}: imported {}, ignored {} duplicates",
                    stats.inserted,
                    stats.ignored
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::OpenMode;

    fn temp_db(dir: &tempfile::TempDir, name: &str) -> Database {
        let path = dir.path().join(name);
        Database::open(&path, OpenMode::Bulk, 1).unwrap()
    }

    fn ts() -> api::Timestamp {
        api::Timestamp::from_jiff(jiff::Timestamp::from_millisecond(1_700_000_000_123).unwrap())
    }

    /// A database populated with one show + remote + watched episode and one
    /// movie + remote + watched movie, using non-default remote attributes so the
    /// round-trip exercises field preservation.
    async fn seed(db: &Database) {
        let root = db.default_owner().await.unwrap();
        let show = api::ShowId::new(1001);
        db.create_show(show, "", None, "").await.unwrap();
        db.import_show_remote(crate::db::ExportRemote {
            id: api::RemoteId::new(9001),
            owner: show,
            source: api::RemoteSource::Tmdb,
            value: api::RemoteValue::Int(1399),
            slug: Some("got".to_owned()),
            enabled: false,
            priority: 7,
            sync_kinds: Some(api::SyncKindSet::from_bits(u32::MAX)),
        })
        .await
        .unwrap();
        db.insert_watched_episode(
            root,
            api::WatchedId::new(5001),
            ts(),
            show,
            api::SeasonNumber::from_ordinal(1),
            1,
        )
        .await
        .unwrap();

        let movie = api::MovieId::new(2001);
        db.create_movie(movie, "", None, "").await.unwrap();
        db.import_movie_remote(crate::db::ExportRemote {
            id: api::RemoteId::new(9002),
            owner: movie,
            source: api::RemoteSource::Tmdb,
            value: api::RemoteValue::Int(550),
            slug: None,
            enabled: true,
            priority: 0,
            sync_kinds: None,
        })
        .await
        .unwrap();
        db.insert_watched_movie(root, api::WatchedId::new(6001), ts(), movie)
            .await
            .unwrap();

        let alice = db
            .create_user(
                "alice",
                Some("alice@example.com"),
                auth::UserRole::Regular,
                api::Timestamp::now(),
            )
            .await
            .unwrap()
            .unwrap()
            .id;
        db.insert_watched_movie(alice, api::WatchedId::new(6002), ts(), movie)
            .await
            .unwrap();

        db.set_show_tracked(root, show, true).await.unwrap();
        db.set_show_tracked(alice, show, true).await.unwrap();
        db.set_movie_tracked(alice, movie, true).await.unwrap();

        let preferences = api::Preferences {
            theme: api::ThemeType::Light,
            ..api::Preferences::default()
        };
        db.save_preferences(root, &preferences).await.unwrap();
        db.set_show_language(alice, show, api::Locale::EN_US)
            .await
            .unwrap();
        db.set_show_include_specials(alice, show, api::IncludeSpecials::Include)
            .await
            .unwrap();
        db.set_movie_language(alice, movie, api::Locale::EN_US)
            .await
            .unwrap();
    }

    fn counts(stats: ImportStats) -> (u64, u64) {
        (stats.inserted, stats.ignored)
    }

    async fn export_to_vec(db: &Database) -> Vec<u8> {
        let mut buf = Vec::new();
        export(db, &mut buf).await.unwrap();
        buf
    }

    #[tokio::test]
    async fn round_trip_preserves_data() {
        let dir = tempfile::tempdir().unwrap();
        let src = temp_db(&dir, "src.db");
        seed(&src).await;
        let exported = export_to_vec(&src).await;

        // Import into a fresh database; ids are inserted as-is (no FK to shows).
        let dst = temp_db(&dir, "dst.db");
        import(&dst, exported.as_slice()).await.unwrap();

        // Re-exporting the destination yields a byte-identical backup.
        let re_exported = export_to_vec(&dst).await;
        let exported = String::from_utf8(exported).unwrap();
        assert_eq!(exported, String::from_utf8(re_exported).unwrap());

        for kind in [
            "user",
            "show_remote",
            "movie_remote",
            "tracked_show",
            "tracked_movie",
            "user_preference",
            "show_preference",
            "movie_preference",
            "watched_episode",
            "watched_movie",
        ] {
            let tag = format!(r#""type":"{kind}""#);
            assert!(exported.contains(&tag), "{tag} in {exported}");
        }

        // Users are restored without a password.
        let alice = dst.user_by_login("alice").await.unwrap().unwrap();
        assert_eq!(alice.email.as_deref(), Some("alice@example.com"));
        assert_eq!(alice.role, auth::UserRole::Regular);
        assert!(alice.password_hash.is_none());

        let root = dst.default_owner().await.unwrap();
        let show = api::ShowId::new(1001);
        let movie = api::MovieId::new(2001);
        assert!(
            dst.show_by_id(Some(alice.id), show)
                .await
                .unwrap()
                .unwrap()
                .tracked
        );
        assert!(
            dst.show_by_id(Some(root), show)
                .await
                .unwrap()
                .unwrap()
                .tracked
        );
        assert!(
            dst.movie_by_id(Some(alice.id), movie)
                .await
                .unwrap()
                .unwrap()
                .tracked
        );
        assert!(
            !dst.movie_by_id(Some(root), movie)
                .await
                .unwrap()
                .unwrap()
                .tracked
        );
        assert_eq!(
            dst.load_preferences(root).await.unwrap().theme,
            api::ThemeType::Light
        );
    }

    /// The watch-next queue is not exported but rebuilt from the imported
    /// tracking and watch history.
    #[tokio::test]
    async fn import_rebuilds_pending() {
        let dir = tempfile::tempdir().unwrap();
        let src = temp_db(&dir, "src.db");
        seed(&src).await;
        let exported = export_to_vec(&src).await;

        let dst = temp_db(&dir, "dst.db");
        let show = api::ShowId::new(1001);
        let season = api::SeasonNumber::from_ordinal(1);
        let e1 = api::EpisodeId::new(11);
        let e2 = api::EpisodeId::new(12);
        dst.create_show(show, "", None, "").await.unwrap();
        dst.upsert_episode(e1, show, season, 1, None, Some(ts()))
            .await
            .unwrap();
        dst.upsert_episode(e2, show, season, 2, None, Some(ts()))
            .await
            .unwrap();

        import(&dst, exported.as_slice()).await.unwrap();

        let pending = |pending: Vec<api::Pending>| {
            pending
                .into_iter()
                .filter_map(|p| match p.info {
                    api::PendingInfo::Episode { episode_id, .. } => Some(episode_id),
                    api::PendingInfo::Movie { .. } => None,
                })
                .collect::<Vec<_>>()
        };

        // Root watched E1 in the backup; alice has not.
        let now = api::Timestamp::now();
        let root = dst.default_owner().await.unwrap();
        let alice = dst.user_by_login("alice").await.unwrap().unwrap().id;
        assert_eq!(pending(dst.pending(root, now).await.unwrap()), [e2]);
        assert_eq!(pending(dst.pending(alice, now).await.unwrap()), [e1]);
    }

    #[tokio::test]
    async fn import_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let src = temp_db(&dir, "src.db");
        seed(&src).await;
        let exported = export_to_vec(&src).await;

        let dst = temp_db(&dir, "dst.db");

        // Root exists in every database; alice is created.
        let first = import(&dst, exported.as_slice()).await.unwrap();
        assert_eq!(counts(first.users), (1, 1));
        assert_eq!(counts(first.remotes), (2, 0));
        assert_eq!(counts(first.tracked), (3, 0));
        assert_eq!(counts(first.preferences), (4, 0));
        assert_eq!(counts(first.watched), (3, 0));

        // Second run inserts nothing; every entry is explicitly ignored.
        let second = import(&dst, exported.as_slice()).await.unwrap();
        assert_eq!(counts(second.users), (0, 2));
        assert_eq!(counts(second.remotes), (0, 2));
        assert_eq!(counts(second.tracked), (0, 3));
        assert_eq!(counts(second.preferences), (0, 4));
        assert_eq!(counts(second.watched), (0, 3));
    }

    /// A remote the destination already has under another identifier is
    /// ignored rather than counted as inserted, and keeps its own settings.
    #[tokio::test]
    async fn import_matches_remotes_by_structure() {
        let dir = tempfile::tempdir().unwrap();
        let src = temp_db(&dir, "src.db");
        seed(&src).await;
        let exported = export_to_vec(&src).await;

        let dst = temp_db(&dir, "dst.db");
        let show = api::ShowId::new(1001);
        dst.create_show(show, "", None, "").await.unwrap();
        dst.add_remote(show, None, &api::Remote::tmdb(1399))
            .await
            .unwrap();

        let report = import(&dst, exported.as_slice()).await.unwrap();
        assert_eq!(counts(report.remotes), (1, 1));

        let remotes = dst.export_snapshot().await.unwrap().show_remotes;
        assert_eq!(remotes.len(), 1);
        assert_ne!(remotes[0].id, api::RemoteId::new(9001));
        assert!(remotes[0].enabled);
    }

    #[tokio::test]
    async fn import_skips_comments_and_blanks() {
        let dir = tempfile::tempdir().unwrap();
        let src = temp_db(&dir, "src.db");
        seed(&src).await;
        let exported = export_to_vec(&src).await;

        // Interleave comments (incl. indented) and blank lines between data rows.
        let mut annotated = String::from("# leading comment\n\n");
        for line in String::from_utf8(exported).unwrap().lines() {
            annotated.push_str(line);
            annotated.push('\n');
            annotated.push_str("   # indented comment\n\n");
        }

        let dst = temp_db(&dir, "dst.db");
        let report = import(&dst, annotated.as_bytes()).await.unwrap();
        assert_eq!((report.remotes.inserted, report.watched.inserted), (2, 3));
    }

    #[tokio::test]
    async fn watched_entries_keep_their_owner() {
        let dir = tempfile::tempdir().unwrap();
        let src = temp_db(&dir, "src.db");
        seed(&src).await;
        let exported = String::from_utf8(export_to_vec(&src).await).unwrap();
        let watched = exported
            .lines()
            .filter(|line| {
                line.contains(r#""user":"root""#) && line.contains(r#""type":"watched_"#)
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(watched.lines().count(), 2, "{exported}");

        // An entry naming a missing user is refused rather than reassigned.
        let dst = temp_db(&dir, "dst.db");
        let to_bob = watched.replace(r#""user":"root""#, r#""user":"bob""#);
        assert!(import(&dst, to_bob.as_bytes()).await.is_err());

        // Entries from before there were users belong to root.
        let unowned = watched.replace(r#""user":"root","#, "");
        let report = import(&dst, unowned.as_bytes()).await.unwrap();
        assert_eq!(report.watched.inserted, 2);
        assert_eq!(
            dst.export_snapshot().await.unwrap().watched_episodes[0].1,
            "root"
        );
    }
}
