//! Backup of the irreplaceable data in the service: the user's remotes and
//! their watched history. Everything else (titles, episodes, images, release
//! dates) is reproducible by re-syncing from the remotes, so it is deliberately
//! not exported.
//!
//! The format is newline-delimited JSON: one [`BackupRow`] per line, tagged by
//! a `type` field. Lines whose first non-whitespace character is `#` are
//! comments and blank lines are ignored, so a backup file can be annotated by
//! hand. Import is idempotent entries that already exist are explicitly counted
//! and skipped.
//!
//! Identifiers are exported as their canonical string form and timestamps as UTC
//! ISO-8601 strings. The id columns in the database are intentionally not foreign
//! keys, so a backup can be inserted (or removed) without touching the associated
//! `shows`/`movies` rows.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use tracing::Level;

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
    /// A single watched episode, keyed by show + season + episode (not episode id,
    /// so it survives a re-sync).
    WatchedEpisode {
        #[serde(with = "as_string")]
        id: api::WatchedId,
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

/// Write every remote and watched entry as a JSON line to `out`.
async fn export(db: &Database, mut out: impl Write) -> Result<()> {
    for r in db.export_show_remotes().await? {
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
        writeln!(out, "{}", serde_json::to_string(&row)?)?;
    }

    for r in db.export_movie_remotes().await? {
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
        writeln!(out, "{}", serde_json::to_string(&row)?)?;
    }

    for (id, timestamp, show, season, episode) in db.export_watched_episodes().await? {
        let row = BackupRow::WatchedEpisode {
            id,
            show,
            season,
            episode,
            timestamp,
        };
        writeln!(out, "{}", serde_json::to_string(&row)?)?;
    }

    for (id, timestamp, movie) in db.export_watched_movies().await? {
        let row = BackupRow::WatchedMovie {
            id,
            movie,
            timestamp,
        };
        writeln!(out, "{}", serde_json::to_string(&row)?)?;
    }

    out.flush()?;
    Ok(())
}

/// Whether a line is a comment (first non-whitespace char is `#`) or blank.
fn is_skippable(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Read JSON lines from `input` and apply them idempotently. Returns the totals
/// for remotes and watched entries.
async fn import(db: &Database, input: impl BufRead) -> Result<(ImportStats, ImportStats)> {
    let mut remotes = ImportStats::default();
    let mut watched = ImportStats::default();

    for (n, line) in input.lines().enumerate() {
        let line = line.with_context(|| anyhow!("Reading line {}", n + 1))?;

        if is_skippable(&line) {
            continue;
        }

        let row: BackupRow = serde_json::from_str(&line)
            .with_context(|| anyhow!("Parsing line {}: {line}", n + 1))?;

        match row {
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
                remotes.record(inserted);
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
                remotes.record(inserted);
            }
            BackupRow::WatchedEpisode {
                id,
                show,
                season,
                episode,
                timestamp,
            } => {
                let inserted = db
                    .import_watched_episode(id, timestamp, show, season, episode)
                    .await?;
                watched.record(inserted);
            }
            BackupRow::WatchedMovie {
                id,
                movie,
                timestamp,
            } => {
                let inserted = db.import_watched_movie(id, timestamp, movie).await?;
                watched.record(inserted);
            }
        }
    }

    Ok((remotes, watched))
}

/// Subcommands of the `track` binary that back up the irreplaceable data.
#[derive(Subcommand)]
pub enum BackupCommand {
    /// Write remotes and watched history as JSON lines.
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

    tracing_subscriber::fmt().with_env_filter(filter).init();

    match command {
        BackupCommand::Export { output } => {
            // Export only reads, via the database's shared (read-only) side.
            let database = Database::open(db, OpenMode::Normal, 1)
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

            let (remotes, watched) = match input {
                Some(path) => {
                    let file = std::fs::File::open(&path)
                        .with_context(|| anyhow!("Opening {}", path.display()))?;
                    import(&database, std::io::BufReader::new(file)).await?
                }
                None => import(&database, std::io::stdin().lock()).await?,
            };

            tracing::info!(
                "Remotes: imported {}, ignored {} duplicates",
                remotes.inserted,
                remotes.ignored
            );
            tracing::info!(
                "Watched: imported {}, ignored {} duplicates",
                watched.inserted,
                watched.ignored
            );
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
            api::WatchedId::new(5001),
            ts(),
            show,
            api::SeasonNumber::from_ordinal(1),
            1,
        )
        .await
        .unwrap();

        let movie = api::MovieId::new(2001);
        db.create_movie(movie, "", None, "", true).await.unwrap();
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
        db.insert_watched_movie(api::WatchedId::new(6001), ts(), movie)
            .await
            .unwrap();
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
        assert_eq!(
            String::from_utf8(exported).unwrap(),
            String::from_utf8(re_exported).unwrap()
        );
    }

    #[tokio::test]
    async fn ids_and_timestamps_are_strings() {
        let dir = tempfile::tempdir().unwrap();
        let src = temp_db(&dir, "fmt.db");
        seed(&src).await;
        let exported = String::from_utf8(export_to_vec(&src).await).unwrap();

        // The opaque id is its base64 string form, and the timestamp is a UTC
        // ISO string ending in `Z`, both quoted JSON strings.
        let episode_line = exported
            .lines()
            .find(|l| l.contains("watched_episode"))
            .unwrap();
        let show_str = api::ShowId::new(1001).to_string();
        assert!(episode_line.contains(&format!("\"show\":\"{show_str}\"")));
        assert!(episode_line.contains("\"timestamp\":\"2023-11-14T22:13:20.123Z\""));

        // sync_kinds is a sequence of strings, not a bitmask.
        let remote_line = exported
            .lines()
            .find(|l| l.contains("show_remote"))
            .unwrap();
        assert!(remote_line.contains("\"sync_kinds\":[\"base\",\"air_date\"]"));
    }

    #[tokio::test]
    async fn import_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let src = temp_db(&dir, "src.db");
        seed(&src).await;
        let exported = export_to_vec(&src).await;

        let dst = temp_db(&dir, "dst.db");

        let (r1, w1) = import(&dst, exported.as_slice()).await.unwrap();
        assert_eq!((r1.inserted, r1.ignored), (2, 0));
        assert_eq!((w1.inserted, w1.ignored), (2, 0));

        // Second run inserts nothing; every entry is explicitly ignored.
        let (r2, w2) = import(&dst, exported.as_slice()).await.unwrap();
        assert_eq!((r2.inserted, r2.ignored), (0, 2));
        assert_eq!((w2.inserted, w2.ignored), (0, 2));
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
        let (remotes, watched) = import(&dst, annotated.as_bytes()).await.unwrap();
        assert_eq!((remotes.inserted, watched.inserted), (2, 2));
    }
}
