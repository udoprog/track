#![allow(clippy::too_many_arguments)]

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use std::collections::HashMap;

use api::{
    Config, Date, EpisodeId, Image, ImageId, ImageKind, ImageSource, MovieId, ReleaseType,
    RemoteId, SeasonId, SeasonNumber, SeriesId, SyncSource, ThemeType, Timestamp, WatchedId,
    WatchedKind,
};
use rust_embed::RustEmbed;
use sqll::{OpenOptions, Row, SendStatement};
use tokio::sync::Mutex;
use tokio::task::spawn_blocking;

const MIGRATIONS_INIT: &str = r#"
CREATE TABLE IF NOT EXISTS migrations (
    id         TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL
);
"#;

#[derive(RustEmbed)]
#[folder = "migrations"]
struct Migrations;

// ── Row types ────────────────────────────────────────────────────────────────

#[derive(Row)]
struct SeriesRow {
    id: SeriesId,
    title: Option<String>,
    first_air: Option<Timestamp>,
    overview: Option<String>,
    tracked: bool,
    sync_source: Option<SyncSource>,
    last_synced_at: Option<Timestamp>,
    language: Option<String>,
}

#[derive(Row)]
struct RemoteRow {
    series_id: Option<SeriesId>,
    movie_id: Option<MovieId>,
    remote_id: RemoteId,
}

#[derive(Row)]
struct ImageRow {
    id: ImageId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    selected: bool,
    series_id: Option<SeriesId>,
    movie_id: Option<MovieId>,
}

#[derive(Row)]
struct SeasonRow {
    id: SeasonId,
    series_id: SeriesId,
    number: u32,
    air_date: Option<Timestamp>,
    name: Option<String>,
    overview: Option<String>,
    poster: Option<Image>,
}

#[derive(Row)]
struct EpisodeRow {
    id: EpisodeId,
    series_id: SeriesId,
    season: u32,
    number: u32,
    absolute_number: Option<u32>,
    name: Option<String>,
    overview: Option<String>,
    aired: Option<Timestamp>,
    filename: Option<Image>,
    remote_id: Option<RemoteId>,
    pending: bool,
}

#[derive(Row)]
struct MovieRow {
    id: MovieId,
    title: Option<String>,
    release_date: Option<Timestamp>,
    overview: Option<String>,
    tracked: bool,
    sync_source: Option<SyncSource>,
    last_synced_at: Option<Timestamp>,
    language: Option<String>,
}

#[derive(Row)]
struct MovieReleaseRow {
    country: String,
    release_type: ReleaseType,
    timestamp: Timestamp,
}

#[derive(Row)]
struct WatchedRow {
    id: WatchedId,
    timestamp: Timestamp,
    episode_id: Option<EpisodeId>,
    movie_id: Option<MovieId>,
    series_id: Option<SeriesId>,
}

#[derive(Row)]
struct WatchedEpisodeRow {
    id: WatchedId,
    timestamp: Timestamp,
    episode_id: EpisodeId,
}

#[derive(Row)]
struct InsertWatchedRow {
    id: WatchedId,
    timestamp: Timestamp,
}

#[derive(Row)]
struct PendingBaseRow {
    episode_id: Option<api::EpisodeId>,
    movie_id: Option<api::MovieId>,
}

#[derive(Row)]
struct PendingEpisodeDetailRow {
    series_id: api::SeriesId,
    series_title: String,
    season: i64,
    number: i64,
    episode_name: Option<String>,
    aired: Option<Timestamp>,
}

#[derive(Row)]
struct PendingMovieDetailRow {
    title: String,
    release_date: Option<Timestamp>,
}

#[derive(Row)]
struct PosterRow {
    source: String,
    path: String,
}

#[derive(Row)]
struct NextEpisodeRow {
    id: api::EpisodeId,
    aired: Option<Timestamp>,
}

#[derive(Row)]
struct PendingMovieCandidateRow {
    id: api::MovieId,
    release_date: Option<Timestamp>,
}

#[derive(Row)]
struct ScheduleRow {
    series_id: SeriesId,
    series_title: String,
    episode_id: EpisodeId,
    season: u32,
    number: u32,
    absolute_number: Option<u32>,
    name: Option<String>,
    overview: Option<String>,
    aired: Option<Timestamp>,
    filename: Option<Image>,
    remote_id: Option<RemoteId>,
}

#[derive(Row)]
struct ConfigRow {
    value: String,
}

// ── Statements ───────────────────────────────────────────────────────────────

macro_rules! statements {
    (
        $vis:vis struct $struct_name:ident {
            $($name:ident: $sql:expr),* $(,)?
        }
    ) => {
        $vis struct $struct_name {
            $($name: SendStatement,)*
        }

        impl $struct_name {
            fn new(c: &sqll::Connection) -> Result<Self> {
                unsafe {
                    Ok(Self {
                        $(
                            $name: c
                                .prepare_with($sql)
                                .persistent()
                                .build()
                                .context(concat!("preparing statement ", stringify!($name)))?
                                .into_send()?,
                        )*
                    })
                }
            }
        }
    }
}

statements! {
    struct Inner {
        // series
        insert_series: r#"
            INSERT INTO series (title, first_air, overview, tracked)
            VALUES (?, ?, ?, ?)
            RETURNING id, title, first_air, overview, tracked, sync_source, last_synced_at, language
        "#,
        list_series: r#"
            SELECT id, title, first_air, overview, tracked, sync_source, last_synced_at, language
            FROM series ORDER BY title
        "#,
        series_by_id: r#"
            SELECT id, title, first_air, overview, tracked, sync_source, last_synced_at, language
            FROM series WHERE id = ?
        "#,
        series_by_remote: r#"
            SELECT s.id, s.title, s.first_air, s.overview, s.tracked, s.sync_source, s.last_synced_at, s.language
            FROM series s
            JOIN remotes r ON r.series_id = s.id
            WHERE r.remote_id = ?
        "#,
        update_series: r#"
            UPDATE series
            SET title = ?, first_air = ?, overview = ?, tracked = ?
            WHERE id = ?
        "#,
        delete_series: r#"
            DELETE FROM series WHERE id = ?
        "#,
        set_series_tracked: r#"
            UPDATE series SET tracked = ? WHERE id = ?
        "#,
        set_series_sync_source: r#"
            UPDATE series SET sync_source = ? WHERE id = ?
        "#,
        set_series_language: r#"
            UPDATE series SET language = ? WHERE id = ?
        "#,

        // remotes (series and movies share one table)
        list_series_remotes: r#"
            SELECT series_id, movie_id, remote_id FROM remotes WHERE series_id = ? ORDER BY id
        "#,
        list_all_series_remotes: r#"
            SELECT series_id, movie_id, remote_id FROM remotes WHERE series_id IS NOT NULL ORDER BY series_id, id
        "#,
        insert_series_remote: r#"
            INSERT OR IGNORE INTO remotes (series_id, remote_id) VALUES (?, ?)
        "#,

        // images (series and movies share one table)
        list_series_images: r#"
            SELECT id, kind, source, path, selected, series_id, movie_id FROM images
            WHERE series_id = ? ORDER BY kind, selected DESC, id
        "#,
        list_all_series_images: r#"
            SELECT id, kind, source, path, selected, series_id, movie_id FROM images
            WHERE series_id IS NOT NULL ORDER BY series_id, kind, selected DESC, id
        "#,
        insert_series_image: r#"
            INSERT INTO images (series_id, kind, source, path, selected) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(series_id, kind, path) WHERE series_id IS NOT NULL DO UPDATE SET source = excluded.source
            RETURNING id, kind, source, path, selected, series_id, movie_id
        "#,
        list_movie_images: r#"
            SELECT id, kind, source, path, selected, series_id, movie_id FROM images
            WHERE movie_id = ? ORDER BY kind, selected DESC, id
        "#,
        list_all_movie_images: r#"
            SELECT id, kind, source, path, selected, series_id, movie_id FROM images
            WHERE movie_id IS NOT NULL ORDER BY movie_id, kind, selected DESC, id
        "#,
        insert_movie_image: r#"
            INSERT INTO images (movie_id, kind, source, path, selected) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(movie_id, kind, path) WHERE movie_id IS NOT NULL DO UPDATE SET source = excluded.source
            RETURNING id, kind, source, path, selected, series_id, movie_id
        "#,
        image_by_id: r#"
            SELECT id, kind, source, path, selected, series_id, movie_id FROM images WHERE id = ?
        "#,
        deselect_series_images: r#"
            UPDATE images SET selected = 0 WHERE series_id = ? AND kind = ?
        "#,
        deselect_movie_images: r#"
            UPDATE images SET selected = 0 WHERE movie_id = ? AND kind = ?
        "#,
        select_image: r#"
            UPDATE images SET selected = 1 WHERE id = ?
        "#,

        // seasons
        upsert_season: r#"
            INSERT INTO seasons (series_id, number, air_date, name, overview, poster)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(series_id, number) DO UPDATE SET
                air_date  = excluded.air_date,
                name      = excluded.name,
                overview  = excluded.overview,
                poster    = excluded.poster
            RETURNING id, series_id, number, air_date, name, overview, poster
        "#,
        list_seasons: r#"
            SELECT id, series_id, number, air_date, name, overview, poster
            FROM seasons WHERE series_id = ? ORDER BY number
        "#,
        delete_season: r#"DELETE FROM seasons WHERE series_id = ?1 AND number = ?2"#,
        delete_season_episodes: r#"DELETE FROM episodes WHERE series_id = ?1 AND season = ?2"#,

        // episodes
        upsert_episode: r#"
            INSERT INTO episodes (series_id, season, number, absolute_number, name, overview, aired, filename, remote_id)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(series_id, season, number) DO UPDATE SET
                absolute_number = excluded.absolute_number,
                name            = excluded.name,
                overview        = excluded.overview,
                aired           = excluded.aired,
                filename        = excluded.filename,
                remote_id       = excluded.remote_id
            RETURNING id, series_id, season, number, absolute_number, name, overview, aired, filename, remote_id, 0 AS pending
        "#,
        list_episodes: r#"
            SELECT e.id, e.series_id, e.season, e.number, e.absolute_number, e.name, e.overview, e.aired, e.filename, e.remote_id,
                   EXISTS(SELECT 1 FROM pending p WHERE p.episode_id = e.id) AS pending
            FROM episodes e
            WHERE e.series_id = ? AND e.season = ?
            ORDER BY e.number
        "#,
        list_episodes_watched: r#"
            SELECT w.id, w.timestamp, w.episode_id
            FROM watched w
            JOIN episodes e ON e.id = w.episode_id
            WHERE e.series_id = ?
            ORDER BY w.timestamp DESC
        "#,
        episode_by_id: r#"
            SELECT e.id, e.series_id, e.season, e.number, e.absolute_number, e.name, e.overview, e.aired, e.filename, e.remote_id,
                   EXISTS(SELECT 1 FROM pending p WHERE p.episode_id = e.id) AS pending
            FROM episodes e WHERE e.id = ?
        "#,
        episode_aired_by_id: r#"
            SELECT aired FROM episodes WHERE id = ?
        "#,
        update_episode_aired: r#"
            UPDATE episodes SET aired = ? WHERE series_id = ? AND season = ? AND number = ?
        "#,

        // movies
        insert_movie: r#"
            INSERT INTO movies (title, release_date, overview, tracked)
            VALUES (?, ?, ?, ?)
            RETURNING id, title, release_date, overview, tracked, sync_source, last_synced_at, language
        "#,
        list_movies: r#"
            SELECT m.id, m.title, m.release_date, m.overview, m.tracked, m.sync_source, m.last_synced_at, m.language
            FROM movies m ORDER BY m.title
        "#,
        movie_by_id: r#"
            SELECT m.id, m.title, m.release_date, m.overview, m.tracked, m.sync_source, m.last_synced_at, m.language
            FROM movies m WHERE m.id = ?
        "#,
        movie_by_remote: r#"
            SELECT m.id, m.title, m.release_date, m.overview, m.tracked, m.sync_source, m.last_synced_at, m.language
            FROM movies m
            JOIN remotes r ON r.movie_id = m.id
            WHERE r.remote_id = ?
        "#,
        set_movie_tracked: r#"
            UPDATE movies SET tracked = ? WHERE id = ?
        "#,
        set_movie_sync_source: r#"
            UPDATE movies SET sync_source = ? WHERE id = ?
        "#,
        set_movie_language: r#"
            UPDATE movies SET language = ? WHERE id = ?
        "#,
        update_movie: r#"
            UPDATE movies
            SET title = ?, release_date = ?, overview = ?, tracked = ?
            WHERE id = ?
        "#,
        delete_movie: r#"
            DELETE FROM movies WHERE id = ?
        "#,
        list_movie_remotes: r#"
            SELECT series_id, movie_id, remote_id FROM remotes WHERE movie_id = ? ORDER BY id
        "#,
        list_all_movie_remotes: r#"
            SELECT series_id, movie_id, remote_id FROM remotes WHERE movie_id IS NOT NULL ORDER BY movie_id, id
        "#,
        insert_movie_remote: r#"
            INSERT OR IGNORE INTO remotes (movie_id, remote_id) VALUES (?, ?)
        "#,

        // watched
        insert_watched: r#"
            INSERT INTO watched (timestamp, episode_id, movie_id)
            VALUES (?, ?, ?)
            RETURNING id, timestamp
        "#,
        delete_watched: r#"
            DELETE FROM watched WHERE id = ?
        "#,
        list_watched_episode: r#"
            SELECT w.id, w.timestamp, w.episode_id, w.movie_id, e.series_id
            FROM watched w JOIN episodes e ON e.id = w.episode_id
            WHERE w.episode_id = ? ORDER BY w.timestamp DESC
        "#,
        list_watched_movie: r#"
            SELECT id, timestamp, episode_id, movie_id, NULL AS series_id
            FROM watched WHERE movie_id = ? ORDER BY timestamp DESC
        "#,

        // pending table management
        upsert_pending_episode: r#"
            INSERT INTO pending (timestamp, series_id, episode_id) VALUES (?, ?, ?)
            ON CONFLICT(series_id) WHERE series_id IS NOT NULL
                DO UPDATE SET episode_id = excluded.episode_id, timestamp = excluded.timestamp
        "#,
        upsert_pending_movie: r#"
            INSERT INTO pending (timestamp, movie_id) VALUES (?, ?)
            ON CONFLICT(movie_id) WHERE movie_id IS NOT NULL
                DO UPDATE SET timestamp = excluded.timestamp
        "#,
        delete_pending_episode: r#"DELETE FROM pending WHERE series_id = ?"#,
        next_episode_after: r#"
            SELECT e.id, e.aired FROM episodes e
            JOIN episodes curr ON curr.id = ?2
            WHERE e.series_id = ?1
              AND (e.season > curr.season OR (e.season = curr.season AND e.number > curr.number))
            ORDER BY e.season, e.number
            LIMIT 1
        "#,
        delete_pending_movie: r#"DELETE FROM pending WHERE movie_id = ?"#,
        has_pending_movie: r#"SELECT 1 FROM pending WHERE movie_id = ? LIMIT 1"#,
        has_pending_episode_for_series: r#"
            SELECT 1 FROM pending WHERE series_id = ? LIMIT 1
        "#,
        next_pending_episode_for_series: r#"
            SELECT e.id, e.aired
            FROM episodes e
            WHERE e.series_id = ?
              AND (e.aired IS NOT NULL AND e.aired <= ?)
              AND NOT EXISTS (SELECT 1 FROM watched w WHERE w.episode_id = e.id)
            ORDER BY e.season, e.number
            LIMIT 1
        "#,
        movies_needing_pending: r#"
            SELECT m.id, m.release_date
            FROM movies m
            WHERE m.tracked = 1
              AND (m.release_date IS NOT NULL AND m.release_date <= ?)
              AND NOT EXISTS (SELECT 1 FROM watched w WHERE w.movie_id = m.id)
              AND NOT EXISTS (SELECT 1 FROM pending p WHERE p.movie_id = m.id)
        "#,
        list_pending_before: r#"
            SELECT episode_id, movie_id
            FROM pending
            WHERE timestamp <= ?
            ORDER BY timestamp DESC
        "#,
        pending_episode_detail: r#"
            SELECT e.series_id, s.title AS series_title, e.season, e.number, e.name AS episode_name, e.aired
            FROM episodes e
            JOIN series s ON s.id = e.series_id
            WHERE e.id = ?
        "#,
        pending_movie_detail: r#"
            SELECT title, release_date FROM movies WHERE id = ?
        "#,
        pending_series_poster: r#"
            SELECT source, path FROM images
            WHERE series_id = ? AND kind = 'poster' AND selected = 1
            LIMIT 1
        "#,
        pending_movie_poster: r#"
            SELECT source, path FROM images
            WHERE movie_id = ? AND kind = 'poster' AND selected = 1
            LIMIT 1
        "#,

        // schedule: episodes airing in the next N days
        list_schedule: r#"
            SELECT e.series_id, s.title AS series_title,
                   e.id AS episode_id, e.season, e.number, e.absolute_number,
                   e.name, e.overview, e.aired, e.filename, e.remote_id
            FROM episodes e
            JOIN series s ON s.id = e.series_id
            WHERE s.tracked = 1
              AND e.aired > ?
              AND e.aired <= ?
            ORDER BY e.aired, s.title, e.season, e.number
        "#,

        // all watched (for import dedup)
        list_all_watched: r#"
            SELECT w.id, w.timestamp, w.episode_id, w.movie_id, e.series_id
            FROM watched w LEFT JOIN episodes e ON e.id = w.episode_id
        "#,

        // config
        get_config: r#"
            SELECT value FROM config WHERE key = ?
        "#,
        set_config: r#"
            INSERT INTO config (key, value) VALUES (?, ?)
            ON CONFLICT (key) DO UPDATE SET value = excluded.value
        "#,

        // last_synced_at stamping
        set_series_synced_at: r#"
            UPDATE series SET last_synced_at = ? WHERE id = ?
        "#,
        set_movie_synced_at: r#"
            UPDATE movies SET last_synced_at = ? WHERE id = ?
        "#,

        // stale-item queries
        series_needing_sync: r#"
            SELECT id, title, first_air, overview, tracked, sync_source, last_synced_at, language
            FROM series
            WHERE tracked = 1
              AND (last_synced_at IS NULL OR last_synced_at < ?)
            ORDER BY last_synced_at IS NOT NULL, last_synced_at
        "#,
        movies_needing_sync: r#"
            SELECT m.id, m.title, m.release_date, m.overview, m.tracked, m.sync_source, m.last_synced_at, m.language
            FROM movies m
            WHERE m.tracked = 1
              AND (m.last_synced_at IS NULL OR m.last_synced_at < ?)
            ORDER BY m.last_synced_at IS NOT NULL, m.last_synced_at
        "#,

        // movie releases
        upsert_movie_release: r#"
            INSERT INTO movie_releases (movie_id, country, release_type, timestamp)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(movie_id, country, release_type)
                DO UPDATE SET timestamp = excluded.timestamp
        "#,
        list_movie_releases: r#"
            SELECT country, release_type, timestamp
            FROM movie_releases
            WHERE movie_id = ?
            ORDER BY timestamp, country, release_type
        "#,
        movie_release_by_type: r#"
            SELECT country, release_type, timestamp
            FROM movie_releases
            WHERE movie_id = ? AND release_type = ?
            ORDER BY timestamp, country, release_type
        "#,

        // digital-release pending discovery
        movies_needing_pending_digital: r#"
            SELECT m.id, MIN(mr.timestamp) AS release_timestamp
            FROM movies m
            JOIN movie_releases mr ON mr.movie_id = m.id AND mr.release_type = 'digital'
            WHERE m.tracked = 1
              AND mr.timestamp <= ?
              AND NOT EXISTS (SELECT 1 FROM watched w WHERE w.movie_id = m.id)
              AND NOT EXISTS (SELECT 1 FROM pending p WHERE p.movie_id = m.id)
            GROUP BY m.id
        "#,
    }
}

// ── Database ─────────────────────────────────────────────────────────────────

pub enum OpenMode {
    /// Full synchronization — safe for the server.
    Normal,
    /// No journaling or fsync — fast for bulk import; not crash-safe.
    Bulk,
}

pub struct Database {
    inner: Arc<Mutex<Inner>>,
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Database {
    pub fn open(path: impl AsRef<Path>, mode: OpenMode) -> Result<Self> {
        let path = path.as_ref();

        let c = OpenOptions::new()
            .extended_result_codes()
            .read_write()
            .create()
            .no_mutex()
            .open(path.as_os_str())
            .with_context(|| path.display().to_string())?;

        do_migrations(&c).context("running migrations")?;
        ensure_mode(&c, mode).context("setting database mode")?;

        let inner = Inner::new(&c).context("preparing statements")?;

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    // ── Series ──

    pub async fn create_series(
        &self,
        title: &str,
        first_air: Option<Timestamp>,
        overview: &str,
    ) -> Result<api::Series> {
        let title = title.to_owned();
        let overview = overview.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.insert_series
                .bind((&title[..], first_air.as_ref(), &overview[..], true))?;

            let r = s
                .insert_series
                .next::<SeriesRow>()?
                .context("insert_series returned no row")?;

            ensure!(s.insert_series.step()?.is_done(), "insert_series");
            Ok(series_from_row(r))
        });

        result.await?
    }

    pub async fn add_series_remote(&self, series_id: SeriesId, remote_id: &RemoteId) -> Result<()> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.insert_series_remote.bind((series_id, &remote_id))?;
            ensure!(
                s.insert_series_remote.step()?.is_done(),
                "insert_series_remote"
            );
            Ok(())
        });

        result.await?
    }

    pub async fn series(&self) -> Result<Vec<api::Series>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.list_series.reset()?;
            let mut out: Vec<api::Series> = Vec::new();
            let mut id_to_idx: HashMap<SeriesId, usize> = HashMap::new();
            while let Some(r) = s.list_series.next::<SeriesRow>()? {
                let idx = out.len();
                id_to_idx.insert(r.id, idx);
                out.push(series_from_row(r));
            }
            s.list_all_series_remotes.reset()?;
            while let Some(r) = s.list_all_series_remotes.next::<RemoteRow>()? {
                if let Some(sid) = r.series_id
                    && let Some(&idx) = id_to_idx.get(&sid)
                {
                    out[idx].remotes.push(r.remote_id);
                }
            }
            s.list_all_series_images.reset()?;
            while let Some(r) = s.list_all_series_images.next::<ImageRow>()? {
                if let Some(sid) = r.series_id
                    && let Some(&idx) = id_to_idx.get(&sid)
                {
                    out[idx].images.push(image_from_row(r));
                }
            }
            Ok(out)
        });

        result.await?
    }

    pub async fn series_by_id(&self, id: SeriesId) -> Result<Option<api::Series>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.series_by_id.bind((id,))?;
            let Some(r) = s.series_by_id.next::<SeriesRow>()? else {
                return Ok(None);
            };
            let mut series = series_from_row(r);
            s.list_series_remotes.bind((id,))?;
            while let Some(r) = s.list_series_remotes.next::<RemoteRow>()? {
                series.remotes.push(r.remote_id);
            }
            s.list_series_images.bind((id,))?;
            while let Some(r) = s.list_series_images.next::<ImageRow>()? {
                series.images.push(image_from_row(r));
            }
            Ok(Some(series))
        });

        result.await?
    }

    pub async fn update_series(
        &self,
        id: SeriesId,
        title: Option<&str>,
        first_air: Option<Timestamp>,
        overview: Option<&str>,
        tracked: bool,
    ) -> Result<()> {
        let title = title.map(str::to_owned);
        let overview = overview.map(str::to_owned);
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.update_series.bind((
                title.as_deref(),
                first_air.as_ref(),
                overview.as_deref(),
                tracked,
                id,
            ))?;
            ensure!(s.update_series.step()?.is_done(), "update_series");
            Ok(())
        });

        result.await?
    }

    pub async fn delete_series(&self, id: SeriesId) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.delete_series.bind((id,))?;
            ensure!(s.delete_series.step()?.is_done(), "delete_series");
            Ok(())
        });

        result.await?
    }

    pub async fn set_series_tracked(&self, id: SeriesId, tracked: bool) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.set_series_tracked.bind((tracked, id))?;
            ensure!(s.set_series_tracked.step()?.is_done(), "set_series_tracked");
            Ok(())
        });

        result.await?
    }

    pub async fn set_series_sync_source(&self, id: SeriesId, source: SyncSource) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.set_series_sync_source.bind((source, id))?;
            ensure!(
                s.set_series_sync_source.step()?.is_done(),
                "set_series_sync_source"
            );
            Ok(())
        });

        result.await?
    }

    pub async fn set_series_language(&self, id: SeriesId, language: Option<String>) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.set_series_language.bind((language.as_deref(), id))?;
            ensure!(
                s.set_series_language.step()?.is_done(),
                "set_series_language"
            );
            Ok(())
        });

        result.await?
    }

    // ── Seasons ──

    pub async fn upsert_season(
        &self,
        series_id: SeriesId,
        number: SeasonNumber,
        air_date: Option<Timestamp>,
        name: Option<&str>,
        overview: &str,
        poster: Option<&Image>,
    ) -> Result<api::Season> {
        let name = name.map(str::to_owned);
        let overview = overview.to_owned();
        let poster = poster.cloned();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.upsert_season.bind((
                series_id,
                number.to_u32(),
                air_date.as_ref(),
                name.as_deref(),
                &overview[..],
                poster.as_ref(),
            ))?;
            let r = s
                .upsert_season
                .next::<SeasonRow>()?
                .context("upsert_season returned no row")?;
            ensure!(s.upsert_season.step()?.is_done(), "upsert_season");
            Ok(season_from_row(r))
        });

        result.await?
    }

    pub async fn seasons(&self, series_id: SeriesId) -> Result<Vec<api::Season>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.list_seasons.bind((series_id,))?;
            let mut out = Vec::new();
            while let Some(r) = s.list_seasons.next::<SeasonRow>()? {
                out.push(season_from_row(r));
            }
            Ok(out)
        });

        result.await?
    }

    pub async fn prune_seasons(
        &self,
        series_id: SeriesId,
        kept: &[SeasonNumber],
    ) -> Result<Vec<SeasonNumber>> {
        let existing = self.seasons(series_id).await?;
        let mut removed = Vec::new();

        for season in existing {
            if kept.contains(&season.number) {
                continue;
            }

            let n = season.number.to_u32();
            let mut s = self.inner.clone().lock_owned().await;

            let result = spawn_blocking(move || {
                s.delete_season_episodes.bind((series_id, n))?;
                ensure!(
                    s.delete_season_episodes.step()?.is_done(),
                    "delete_season_episodes"
                );

                s.delete_season.bind((series_id, n))?;
                ensure!(s.delete_season.step()?.is_done(), "delete_season");
                Ok(())
            });

            result.await??;
            removed.push(season.number);
        }

        Ok(removed)
    }

    // ── Episodes ──

    pub async fn upsert_episode(
        &self,
        series_id: SeriesId,
        season: SeasonNumber,
        number: u32,
        absolute_number: Option<u32>,
        name: Option<&str>,
        overview: &str,
        aired: Option<Timestamp>,
        filename: Option<&Image>,
        remote_id: Option<&RemoteId>,
    ) -> Result<api::Episode> {
        let name = name.map(str::to_owned);
        let overview = overview.to_owned();
        let filename = filename.cloned();
        let remote_id = remote_id.cloned();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.upsert_episode.bind((
                series_id,
                season.to_u32(),
                number as i64,
                absolute_number.map(|n| n as i64),
                name.as_deref(),
                &overview[..],
                aired.as_ref(),
                filename.as_ref(),
                remote_id.as_ref(),
            ))?;
            let r = s
                .upsert_episode
                .next::<EpisodeRow>()?
                .context("upsert_episode returned no row")?;
            ensure!(s.upsert_episode.step()?.is_done(), "upsert_episode");
            Ok(episode_from_row(r))
        });

        result.await?
    }

    pub async fn episodes(
        &self,
        series_id: SeriesId,
        season: SeasonNumber,
    ) -> Result<Vec<api::Episode>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.list_episodes.bind((series_id, season.to_u32()))?;
            let mut out = Vec::new();

            while let Some(r) = s.list_episodes.next::<EpisodeRow>()? {
                out.push(episode_from_row(r));
            }

            Ok(out)
        });

        result.await?
    }

    pub async fn episodes_watched(&self, series_id: SeriesId) -> Result<Vec<api::WatchedEpisode>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.list_episodes_watched.bind((series_id,))?;
            let mut out = Vec::new();

            while let Some(r) = s.list_episodes_watched.next::<WatchedEpisodeRow>()? {
                out.push(watched_episode_from_row(r));
            }

            Ok(out)
        });

        result.await?
    }

    pub async fn episode_by_id(&self, id: EpisodeId) -> Result<Option<api::Episode>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.episode_by_id.bind((id,))?;
            Ok(s.episode_by_id.next::<EpisodeRow>()?.map(episode_from_row))
        });

        result.await?
    }

    pub async fn episode_aired_by_id(&self, id: EpisodeId) -> Result<Option<Timestamp>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.episode_aired_by_id.bind((id,))?;
            Ok(s.episode_aired_by_id.next::<Option<Timestamp>>()?.flatten())
        });

        result.await?
    }

    pub async fn update_episodes_aired(
        &self,
        series_id: SeriesId,
        updates: Vec<(SeasonNumber, u32, Timestamp)>,
    ) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            for &(season, number, aired) in &updates {
                s.update_episode_aired
                    .bind((aired, series_id, season.to_u32(), number as i64))?;
                ensure!(
                    s.update_episode_aired.step()?.is_done(),
                    "update_episode_aired"
                );
            }
            Ok(())
        });

        result.await?
    }

    // ── Movies ──

    pub async fn create_movie(
        &self,
        title: &str,
        release_date: Option<Timestamp>,
        overview: &str,
        tracked: bool,
    ) -> Result<api::Movie> {
        let title = title.to_owned();
        let overview = overview.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.insert_movie
                .bind((&title[..], release_date, &overview[..], tracked))?;

            let r = s
                .insert_movie
                .next::<MovieRow>()?
                .context("insert_movie returned no row")?;

            ensure!(s.insert_movie.step()?.is_done(), "insert_movie");
            Ok(movie_from_row(r))
        });

        result.await?
    }

    pub async fn add_movie_remote(&self, movie_id: MovieId, remote_id: &RemoteId) -> Result<()> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.insert_movie_remote.bind((movie_id, &remote_id))?;
            ensure!(
                s.insert_movie_remote.step()?.is_done(),
                "insert_movie_remote"
            );
            Ok(())
        });

        result.await?
    }

    pub async fn movies(&self) -> Result<Vec<api::Movie>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.list_movies.reset()?;
            let mut out: Vec<api::Movie> = Vec::new();
            let mut id_to_idx: HashMap<MovieId, usize> = HashMap::new();
            while let Some(r) = s.list_movies.next::<MovieRow>()? {
                let idx = out.len();
                id_to_idx.insert(r.id, idx);
                out.push(movie_from_row(r));
            }
            s.list_all_movie_remotes.reset()?;
            while let Some(r) = s.list_all_movie_remotes.next::<RemoteRow>()? {
                if let Some(mid) = r.movie_id
                    && let Some(&idx) = id_to_idx.get(&mid)
                {
                    out[idx].remotes.push(r.remote_id);
                }
            }
            s.list_all_movie_images.reset()?;
            while let Some(r) = s.list_all_movie_images.next::<ImageRow>()? {
                if let Some(mid) = r.movie_id
                    && let Some(&idx) = id_to_idx.get(&mid)
                {
                    out[idx].images.push(image_from_row(r));
                }
            }
            Ok(out)
        });

        result.await?
    }

    pub async fn movie_by_id(&self, id: MovieId) -> Result<Option<api::Movie>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.movie_by_id.bind((id,))?;
            let Some(r) = s.movie_by_id.next::<MovieRow>()? else {
                return Ok(None);
            };
            let movie_id = r.id;
            let mut movie = movie_from_row(r);

            s.list_movie_remotes.bind((movie_id,))?;
            while let Some(r) = s.list_movie_remotes.next::<RemoteRow>()? {
                movie.remotes.push(r.remote_id);
            }

            s.list_movie_images.bind((movie_id,))?;
            while let Some(r) = s.list_movie_images.next::<ImageRow>()? {
                movie.images.push(image_from_row(r));
            }

            s.list_movie_releases.bind((movie_id,))?;
            while let Some(r) = s.list_movie_releases.next::<MovieReleaseRow>()? {
                movie.releases.push(api::MovieRelease {
                    country: r.country,
                    release_type: r.release_type,
                    timestamp: r.timestamp,
                });
            }

            s.has_pending_movie.bind((movie_id,))?;
            movie.pending = s.has_pending_movie.next::<(i64,)>()?.is_some();
            Ok(Some(movie))
        });

        result.await?
    }

    pub async fn movie_release_by_type(
        &self,
        id: MovieId,
        ty: ReleaseType,
    ) -> Result<Option<Timestamp>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.movie_release_by_type.bind((id, ty))?;

            let Some(timestamp) = s.movie_release_by_type.next::<Option<Timestamp>>()? else {
                return Ok(None);
            };

            Ok(timestamp)
        });

        result.await?
    }

    pub async fn series_by_remote_id(&self, remote_id: &RemoteId) -> Result<Option<api::Series>> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.series_by_remote.bind((remote_id,))?;
            let Some(r) = s.series_by_remote.next::<SeriesRow>()? else {
                return Ok(None);
            };
            let series_id = r.id;
            let mut series = series_from_row(r);
            s.list_series_remotes.bind((series_id,))?;
            while let Some(r) = s.list_series_remotes.next::<RemoteRow>()? {
                series.remotes.push(r.remote_id);
            }
            s.list_series_images.bind((series_id,))?;
            while let Some(r) = s.list_series_images.next::<ImageRow>()? {
                series.images.push(image_from_row(r));
            }
            Ok(Some(series))
        });

        result.await?
    }

    pub async fn movie_by_remote_id(&self, remote_id: &RemoteId) -> Result<Option<api::Movie>> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.movie_by_remote.bind((remote_id,))?;
            let Some(r) = s.movie_by_remote.next::<MovieRow>()? else {
                return Ok(None);
            };

            let movie_id = r.id;
            let mut movie = movie_from_row(r);
            s.list_movie_remotes.bind((movie_id,))?;
            while let Some(r) = s.list_movie_remotes.next::<RemoteRow>()? {
                movie.remotes.push(r.remote_id);
            }

            s.list_movie_images.bind((movie_id,))?;
            while let Some(r) = s.list_movie_images.next::<ImageRow>()? {
                movie.images.push(image_from_row(r));
            }

            Ok(Some(movie))
        });

        result.await?
    }

    pub async fn update_movie(
        &self,
        id: MovieId,
        title: Option<&str>,
        release_date: Option<Timestamp>,
        overview: Option<&str>,
    ) -> Result<()> {
        let title = title.map(str::to_owned);
        let overview = overview.map(str::to_owned);
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.update_movie.bind((
                title.as_deref(),
                release_date.as_ref(),
                overview.as_deref(),
                id,
            ))?;
            ensure!(s.update_movie.step()?.is_done(), "update_movie");
            Ok(())
        });

        result.await?
    }

    pub async fn delete_movie(&self, id: MovieId) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.delete_movie.bind((id,))?;
            ensure!(s.delete_movie.step()?.is_done(), "delete_movie");
            Ok(())
        });

        result.await?
    }

    pub async fn set_movie_tracked(&self, id: MovieId, tracked: bool) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.set_movie_tracked.bind((tracked, id))?;
            ensure!(s.set_movie_tracked.step()?.is_done(), "set_movie_tracked");
            Ok(())
        });

        result.await?
    }

    pub async fn set_movie_sync_source(&self, id: MovieId, source: SyncSource) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.set_movie_sync_source.bind((source, id))?;
            ensure!(
                s.set_movie_sync_source.step()?.is_done(),
                "set_movie_sync_source"
            );
            Ok(())
        });

        result.await?
    }

    pub async fn set_movie_language(&self, id: MovieId, language: Option<String>) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.set_movie_language.bind((language.as_deref(), id))?;
            ensure!(s.set_movie_language.step()?.is_done(), "set_movie_language");
            Ok(())
        });

        result.await?
    }

    // ── Images ──

    pub async fn upsert_series_image(
        &self,
        series_id: SeriesId,
        kind: ImageKind,
        source: ImageSource,
        path: &str,
    ) -> Result<api::MediaImage> {
        let path = path.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            let is_first = {
                s.list_series_images.bind((series_id,))?;
                let mut found = false;

                while let Some(r) = s.list_series_images.next::<ImageRow>()? {
                    if r.kind == kind {
                        found = true;
                        break;
                    }
                }

                !found
            };

            s.insert_series_image
                .bind((series_id, kind, source, &path[..], is_first))?;
            let r = s
                .insert_series_image
                .next::<ImageRow>()?
                .context("insert_series_image returned no row")?;
            ensure!(
                s.insert_series_image.step()?.is_done(),
                "insert_series_image"
            );

            Ok(image_from_row(r))
        });

        result.await?
    }

    pub async fn upsert_movie_image(
        &self,
        movie_id: MovieId,
        kind: ImageKind,
        source: ImageSource,
        path: &str,
    ) -> Result<api::MediaImage> {
        let path = path.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            let is_first = {
                s.list_movie_images.bind((movie_id,))?;
                let mut found = false;
                while let Some(r) = s.list_movie_images.next::<ImageRow>()? {
                    if r.kind == kind {
                        found = true;
                        break;
                    }
                }
                !found
            };
            s.insert_movie_image
                .bind((movie_id, kind, source, &path[..], is_first))?;
            let r = s
                .insert_movie_image
                .next::<ImageRow>()?
                .context("insert_movie_image returned no row")?;
            ensure!(s.insert_movie_image.step()?.is_done(), "insert_movie_image");
            Ok(image_from_row(r))
        });

        result.await?
    }

    /// Deselects all images of the same kind for the owning entity, then
    /// selects the given image. Returns which entity owns the image.
    pub async fn select_image(&self, id: ImageId) -> Result<api::ImageOwner> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.image_by_id.bind((id,))?;
            let r = s
                .image_by_id
                .next::<ImageRow>()?
                .context("image not found")?;
            let kind = r.kind;
            let owner = match (r.series_id, r.movie_id) {
                (Some(sid), _) => {
                    s.deselect_series_images.bind((sid, kind))?;
                    ensure!(
                        s.deselect_series_images.step()?.is_done(),
                        "deselect_series_images"
                    );
                    api::ImageOwner::Series(sid)
                }
                (_, Some(mid)) => {
                    s.deselect_movie_images.bind((mid, kind))?;
                    ensure!(
                        s.deselect_movie_images.step()?.is_done(),
                        "deselect_movie_images"
                    );
                    api::ImageOwner::Movie(mid)
                }
                _ => anyhow::bail!("image has no owner"),
            };
            s.select_image.bind((id,))?;
            ensure!(s.select_image.step()?.is_done(), "select_image");
            Ok(owner)
        });

        result.await?
    }

    pub async fn clear_selected_image(
        &self,
        owner: api::ImageOwner,
        kind: ImageKind,
    ) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            match owner {
                api::ImageOwner::Series(series_id) => {
                    s.deselect_series_images.bind((series_id, kind))?;
                    ensure!(
                        s.deselect_series_images.step()?.is_done(),
                        "deselect_series_images"
                    );
                }
                api::ImageOwner::Movie(movie_id) => {
                    s.deselect_movie_images.bind((movie_id, kind))?;
                    ensure!(
                        s.deselect_movie_images.step()?.is_done(),
                        "deselect_movie_images"
                    );
                }
            }

            Ok(())
        });

        result.await?
    }

    // ── Watched ──

    pub async fn mark_watched(
        &self,
        kind: WatchedKind,
        timestamp: Timestamp,
    ) -> Result<api::Watched> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            let (episode_id, movie_id) = match kind {
                WatchedKind::Episode { episode, .. } => (Some(episode), None),
                WatchedKind::Movie { movie } => (None, Some(movie)),
            };

            s.insert_watched.bind((timestamp, episode_id, movie_id))?;
            let r = s
                .insert_watched
                .next::<InsertWatchedRow>()?
                .context("insert_watched returned no row")?;
            ensure!(s.insert_watched.step()?.is_done(), "insert_watched");

            Ok(api::Watched {
                id: r.id,
                timestamp: r.timestamp,
                kind,
            })
        });

        result.await?
    }

    pub async fn remove_watched(&self, id: WatchedId) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.delete_watched.bind((id,))?;
            ensure!(s.delete_watched.step()?.is_done(), "delete_watched");
            Ok(())
        });

        result.await?
    }

    pub async fn all_watched(&self) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.list_all_watched.reset()?;
            let mut out = Vec::new();
            while let Some(r) = s.list_all_watched.next::<WatchedRow>()? {
                out.push(watched_from_row(r)?);
            }
            Ok(out)
        });

        result.await?
    }

    pub async fn watched_for_episode(&self, episode_id: EpisodeId) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.list_watched_episode.bind((episode_id,))?;
            let mut out = Vec::new();
            while let Some(r) = s.list_watched_episode.next::<WatchedRow>()? {
                out.push(watched_from_row(r)?);
            }
            Ok(out)
        });

        result.await?
    }

    pub async fn watched_for_movie(&self, movie_id: MovieId) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.list_watched_movie.bind((movie_id,))?;
            let mut out = Vec::new();
            while let Some(r) = s.list_watched_movie.next::<WatchedRow>()? {
                out.push(watched_from_row(r)?);
            }
            Ok(out)
        });

        result.await?
    }

    // ── Pending table ──

    pub async fn add_pending_episode(
        &self,
        series_id: api::SeriesId,
        episode_id: api::EpisodeId,
        ts: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.upsert_pending_episode.bind((ts, series_id, episode_id))?;
            ensure!(
                s.upsert_pending_episode.step()?.is_done(),
                "upsert_pending_episode"
            );
            Ok(())
        });

        result.await?
    }

    pub async fn add_pending_movie(&self, movie_id: api::MovieId, ts: Timestamp) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.upsert_pending_movie.bind((ts, movie_id))?;
            ensure!(
                s.upsert_pending_movie.step()?.is_done(),
                "upsert_pending_movie"
            );
            Ok(())
        });

        result.await?
    }

    pub async fn remove_pending_episode(&self, series_id: api::SeriesId) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.delete_pending_episode.bind((series_id,))?;
            ensure!(
                s.delete_pending_episode.step()?.is_done(),
                "delete_pending_episode"
            );
            Ok(())
        });

        result.await?
    }

    pub async fn skip_pending_episode(
        &self,
        series_id: api::SeriesId,
        episode_id: api::EpisodeId,
    ) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.next_episode_after.bind((series_id, episode_id))?;

            let next_id = s
                .next_episode_after
                .next::<(api::EpisodeId, Timestamp)>()?
                .map(|r| r.0);

            match next_id {
                Some(next) => {
                    let ts = Timestamp::now();
                    s.upsert_pending_episode.bind((ts, series_id, next))?;
                    ensure!(
                        s.upsert_pending_episode.step()?.is_done(),
                        "upsert_pending_episode"
                    );
                }
                None => {
                    s.delete_pending_episode.bind((series_id,))?;
                    ensure!(
                        s.delete_pending_episode.step()?.is_done(),
                        "delete_pending_episode"
                    );
                }
            }

            Ok(())
        });

        result.await?
    }

    pub async fn remove_pending_movie(&self, movie_id: api::MovieId) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.delete_pending_movie.bind((movie_id,))?;
            ensure!(
                s.delete_pending_movie.step()?.is_done(),
                "delete_pending_movie"
            );
            Ok(())
        });

        result.await?
    }

    /// Fill the pending slot for a series, but ONLY if it currently has no pending episode.
    /// Called after sync upserts episodes, and after MarkWatched clears the old pending row.
    pub async fn fill_pending_for_series(
        &self,
        series_id: api::SeriesId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.has_pending_episode_for_series.bind((series_id,))?;
            let already_has = s.has_pending_episode_for_series.next::<(i64,)>()?.is_some();

            if already_has {
                return Ok(());
            }

            s.next_pending_episode_for_series.bind((series_id, now))?;

            let Some(row) = s.next_pending_episode_for_series.next::<NextEpisodeRow>()? else {
                return Ok(());
            };

            let now = row.aired.unwrap_or(now).max(now);

            s.upsert_pending_episode.bind((now, series_id, row.id))?;
            ensure!(
                s.upsert_pending_episode.step()?.is_done(),
                "upsert_pending_episode"
            );

            Ok(())
        });

        result.await?
    }

    /// Fill the pending slot for a series, but ONLY if it currently has no pending episode.
    /// Called after sync upserts episodes, and after MarkWatched clears the old pending row.
    pub async fn fill_pending_for_series_from(
        &self,
        series_id: api::SeriesId,
        episode_id: api::EpisodeId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.has_pending_episode_for_series.bind((series_id,))?;
            let already_has = s.has_pending_episode_for_series.next::<(i64,)>()?.is_some();

            if already_has {
                return Ok(());
            }

            s.next_episode_after.bind((series_id, episode_id))?;

            let Some((next_id, aired)) = s
                .next_episode_after
                .next::<(api::EpisodeId, Option<Timestamp>)>()?
            else {
                return Ok(());
            };

            let now = aired.unwrap_or(now).max(now);

            s.upsert_pending_episode.bind((now, series_id, next_id))?;
            ensure!(
                s.upsert_pending_episode.step()?.is_done(),
                "upsert_pending_episode"
            );

            Ok(())
        });

        result.await?
    }

    /// Tracked movies with a passed theatrical release date that are not yet pending or watched.
    pub async fn theatrical_movie_candidates(
        &self,
        now: Timestamp,
    ) -> Result<Vec<(MovieId, Option<Timestamp>)>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.movies_needing_pending.bind((now,))?;

            let mut out = Vec::new();

            while let Some(r) = s
                .movies_needing_pending
                .next::<PendingMovieCandidateRow>()?
            {
                out.push((r.id, r.release_date));
            }

            Ok(out)
        });

        result.await?
    }

    /// Tracked movies with a passed digital release date (type 4) that are not yet pending or watched.
    pub async fn digital_movie_candidates(
        &self,
        now: Timestamp,
    ) -> Result<Vec<(MovieId, Option<Timestamp>)>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.movies_needing_pending_digital.bind((now,))?;

            let mut out = Vec::new();

            while let Some(r) = s
                .movies_needing_pending_digital
                .next::<PendingMovieCandidateRow>()?
            {
                out.push((r.id, r.release_date));
            }

            Ok(out)
        });

        result.await?
    }

    pub async fn set_series_synced_at(&self, id: SeriesId, at: Timestamp) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;
        let result = spawn_blocking(move || {
            s.set_series_synced_at.bind((at, id))?;
            ensure!(
                s.set_series_synced_at.step()?.is_done(),
                "set_series_synced_at"
            );
            Ok(())
        });

        result.await?
    }

    pub async fn set_movie_synced_at(&self, id: MovieId, at: Timestamp) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;
        let result = spawn_blocking(move || {
            s.set_movie_synced_at.bind((at, id))?;
            ensure!(
                s.set_movie_synced_at.step()?.is_done(),
                "set_movie_synced_at"
            );
            Ok(())
        });

        result.await?
    }

    pub async fn upsert_movie_release(
        &self,
        movie_id: MovieId,
        country: &str,
        release_type: ReleaseType,
        timestamp: &Timestamp,
    ) -> Result<()> {
        let country = country.to_owned();
        let timestamp = *timestamp;
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.upsert_movie_release
                .bind((movie_id, country.as_str(), release_type, timestamp))?;
            ensure!(
                s.upsert_movie_release.step()?.is_done(),
                "upsert_movie_release"
            );

            Ok(())
        });

        result.await?
    }

    pub async fn series_needing_sync(&self, interval_hours: u32) -> Result<Vec<api::Series>> {
        let cutoff = cutoff_timestamp(interval_hours);
        let mut s = self.inner.clone().lock_owned().await;
        let result = spawn_blocking(move || {
            s.series_needing_sync.bind((cutoff,))?;
            let mut out = Vec::new();

            while let Some(r) = s.series_needing_sync.next::<SeriesRow>()? {
                out.push(series_from_row(r));
            }

            Ok(out)
        });

        result.await?
    }

    pub async fn movies_needing_sync(&self, interval_hours: u32) -> Result<Vec<api::Movie>> {
        let cutoff = cutoff_timestamp(interval_hours);
        let mut s = self.inner.clone().lock_owned().await;
        let result = spawn_blocking(move || {
            s.movies_needing_sync.bind((cutoff,))?;
            let mut out = Vec::new();
            while let Some(r) = s.movies_needing_sync.next::<MovieRow>()? {
                out.push(movie_from_row(r));
            }
            Ok(out)
        });

        result.await?
    }

    /// Unified pending list replacing pending_episodes + pending_movies.
    pub async fn pending(&self, now: Timestamp) -> Result<Vec<api::Pending>> {
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            s.list_pending_before.bind((now,))?;

            'outer: while let Some(r) = s.list_pending_before.next::<PendingBaseRow>()? {
                let pending = 'pending: {
                    if let Some(episode_id) = r.episode_id {
                        s.pending_episode_detail.bind((episode_id,))?;
                        let detail = s.pending_episode_detail.next::<PendingEpisodeDetailRow>()?;

                        let Some(d) = detail else {
                            continue 'outer;
                        };

                        s.pending_series_poster.bind((d.series_id,))?;
                        let poster_row = s.pending_series_poster.next::<PosterRow>()?;
                        let poster = poster_row
                            .map(|p| api::Image::from_raw(format!("{}:{}", p.source, p.path)));

                        let label = match d.episode_name {
                            Some(ref name) => {
                                format!("S{:02}E{:02} \u{2013} {name}", d.season, d.number)
                            }
                            None => format!("S{:02}E{:02}", d.season, d.number),
                        };

                        break 'pending api::Pending {
                            kind: api::PendingKind::Episode {
                                series: d.series_id,
                                episode: episode_id,
                            },
                            aired: d.aired,
                            series_title: Some(d.series_title),
                            label,
                            poster,
                        };
                    }

                    if let Some(movie_id) = r.movie_id {
                        s.pending_movie_detail.bind((movie_id,))?;
                        let detail = s.pending_movie_detail.next::<PendingMovieDetailRow>()?;
                        let Some(d) = detail else {
                            continue 'outer;
                        };

                        s.pending_movie_poster.bind((movie_id,))?;
                        let poster_row = s.pending_movie_poster.next::<PosterRow>()?;
                        let poster = poster_row
                            .map(|p| api::Image::from_raw(format!("{}:{}", p.source, p.path)));

                        break 'pending api::Pending {
                            kind: api::PendingKind::Movie { movie: movie_id },
                            aired: d.release_date,
                            series_title: None,
                            label: d.title,
                            poster,
                        };
                    }

                    continue 'outer;
                };

                out.push(pending);
            }

            Ok(out)
        });

        result.await?
    }

    // ── Dashboard queries ──

    pub async fn schedule(
        &self,
        days: u32,
        now: Timestamp,
        tz: api::TimeZone,
    ) -> Result<Vec<api::ScheduledDay>> {
        let today = now.date(tz.clone());

        let Some(end) = today.checked_add_days(days) else {
            return Ok(vec![]);
        };

        let end = end.to_timestamp_at_midnight_zoned(tz.clone())?;

        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.list_schedule.bind((today, end))?;

            let mut days_map = Vec::<(Date, Vec<(SeriesId, String, Vec<api::Episode>)>)>::new();

            while let Some(r) = s.list_schedule.next::<ScheduleRow>()? {
                let Some(day) = r.aired else { continue };

                let ep = api::Episode {
                    id: r.episode_id,
                    series_id: r.series_id,
                    season: SeasonNumber::from_u32(r.season),
                    number: r.number as u32,
                    absolute_number: r.absolute_number,
                    name: r.name,
                    overview: r.overview,
                    aired: r.aired,
                    filename: r.filename,
                    remote_id: r.remote_id,
                    pending: false,
                };

                let day = day.date(tz.clone());

                if let Some(day_entry) = days_map.iter_mut().find(|(d, _)| d == &day) {
                    if let Some(series_entry) =
                        day_entry.1.iter_mut().find(|(id, _, _)| *id == r.series_id)
                    {
                        series_entry.2.push(ep);
                    } else {
                        day_entry.1.push((r.series_id, r.series_title, vec![ep]));
                    }
                } else {
                    days_map.push((day, vec![(r.series_id, r.series_title, vec![ep])]));
                }
            }

            let out = days_map
                .into_iter()
                .map(|(date, series)| api::ScheduledDay {
                    date,
                    entries: series
                        .into_iter()
                        .map(|(series_id, series_title, episodes)| api::ScheduledEntry {
                            series_id,
                            series_title,
                            episodes,
                        })
                        .collect(),
                })
                .collect();

            Ok(out)
        });

        result.await?
    }

    // ── Config ──

    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.get_config.bind((key.as_str(),))?;
            Ok(s.get_config.next::<ConfigRow>()?.map(|r| r.value))
        });

        result.await?
    }

    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let key = key.to_owned();
        let value = value.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        let result = spawn_blocking(move || {
            s.set_config.bind((key.as_str(), value.as_str()))?;
            ensure!(s.set_config.step()?.is_done(), "set_config");
            Ok(())
        });

        result.await?
    }

    pub async fn load_config(&self) -> Result<Config> {
        let theme = self
            .get_config("theme")
            .await?
            .and_then(|v| match v.as_str() {
                "dark" => Some(ThemeType::Dark),
                "light" => Some(ThemeType::Light),
                _ => None,
            })
            .unwrap_or_default();

        let tvdb_legacy_apikey = self
            .get_config("tvdb_legacy_apikey")
            .await?
            .unwrap_or_default();

        let tmdb_api_key = self.get_config("tmdb_api_key").await?.unwrap_or_default();

        let schedule_duration_days = self
            .get_config("schedule_duration_days")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(7);

        let dashboard_limit = self
            .get_config("dashboard_limit")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(6);

        let dashboard_page = self
            .get_config("dashboard_page")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(6);

        let auto_sync_enabled = self
            .get_config("auto_sync_enabled")
            .await?
            .map(|v| v == "true")
            .unwrap_or(false);

        let auto_sync_interval_hours = self
            .get_config("auto_sync_interval_hours")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(24);

        let timezone = self.get_config("timezone").await?.unwrap_or_default();

        let language = self.get_config("language").await?.filter(|v| !v.is_empty());

        Ok(Config {
            theme,
            tvdb_legacy_apikey,
            tmdb_api_key,
            schedule_duration_days,
            dashboard_limit,
            dashboard_page,
            auto_sync_enabled,
            auto_sync_interval_hours,
            timezone,
            language,
        })
    }

    pub async fn save_config(&self, config: &Config) -> Result<()> {
        self.set_config("theme", config.theme.to_string().as_str())
            .await?;

        self.set_config("tvdb_legacy_apikey", &config.tvdb_legacy_apikey)
            .await?;

        self.set_config("tmdb_api_key", &config.tmdb_api_key)
            .await?;

        self.set_config(
            "schedule_duration_days",
            &config.schedule_duration_days.to_string(),
        )
        .await?;

        self.set_config("dashboard_limit", &config.dashboard_limit.to_string())
            .await?;

        self.set_config("dashboard_page", &config.dashboard_page.to_string())
            .await?;

        self.set_config(
            "auto_sync_enabled",
            if config.auto_sync_enabled {
                "true"
            } else {
                "false"
            },
        )
        .await?;

        self.set_config(
            "auto_sync_interval_hours",
            &config.auto_sync_interval_hours.to_string(),
        )
        .await?;

        self.set_config("timezone", &config.timezone).await?;

        self.set_config("language", config.language.as_deref().unwrap_or(""))
            .await?;
        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn cutoff_timestamp(interval_hours: u32) -> Timestamp {
    let hours = interval_hours.max(1) as i64;
    let inner = Timestamp::now().inner();
    let ts = inner
        .checked_sub(jiff::Span::new().hours(hours))
        .unwrap_or(inner);
    Timestamp::from_jiff(ts)
}

// ── Row converters ───────────────────────────────────────────────────────────

fn series_from_row(r: SeriesRow) -> api::Series {
    api::Series {
        id: r.id,
        title: r.title,
        first_air_date: r.first_air,
        overview: r.overview,
        tracked: r.tracked,
        sync_source: r.sync_source,
        remotes: Vec::new(),
        images: Vec::new(),
        last_synced_at: r.last_synced_at,
        language: r.language,
    }
}

fn image_from_row(r: ImageRow) -> api::MediaImage {
    api::MediaImage {
        id: r.id,
        kind: r.kind,
        source: r.source,
        image: Image::from_raw(format!("{}:{}", r.source.as_str(), r.path)),
        selected: r.selected,
    }
}

fn season_from_row(r: SeasonRow) -> api::Season {
    api::Season {
        id: r.id,
        series_id: r.series_id,
        number: SeasonNumber::from_u32(r.number),
        air_date: r.air_date,
        name: r.name,
        overview: r.overview,
        poster: r.poster,
    }
}

fn episode_from_row(r: EpisodeRow) -> api::Episode {
    api::Episode {
        id: r.id,
        series_id: r.series_id,
        season: SeasonNumber::from_u32(r.season),
        number: r.number as u32,
        absolute_number: r.absolute_number,
        name: r.name,
        overview: r.overview,
        aired: r.aired,
        filename: r.filename,
        remote_id: r.remote_id,
        pending: r.pending,
    }
}

fn watched_episode_from_row(r: WatchedEpisodeRow) -> api::WatchedEpisode {
    api::WatchedEpisode {
        id: r.id,
        timestamp: r.timestamp,
        episode_id: r.episode_id,
    }
}

fn movie_from_row(r: MovieRow) -> api::Movie {
    api::Movie {
        id: r.id,
        title: r.title,
        release_date: r.release_date,
        overview: r.overview,
        remotes: Vec::new(),
        sync_source: r.sync_source,
        tracked: r.tracked,
        pending: false,
        images: Vec::new(),
        last_synced_at: r.last_synced_at,
        releases: Vec::new(),
        language: r.language,
    }
}

fn watched_from_row(r: WatchedRow) -> Result<api::Watched> {
    let kind = match (r.series_id, r.episode_id, r.movie_id) {
        (Some(series), Some(episode), None) => WatchedKind::Episode { series, episode },
        (None, None, Some(movie)) => WatchedKind::Movie { movie },
        _ => anyhow::bail!("watched row violates CHECK constraint"),
    };

    Ok(api::Watched {
        id: r.id,
        timestamp: r.timestamp,
        kind,
    })
}

// ── Migrations ───────────────────────────────────────────────────────────────

fn do_migrations(c: &sqll::Connection) -> Result<()> {
    c.execute(MIGRATIONS_INIT)?;

    let mut select = c.prepare("SELECT applied_at FROM migrations WHERE id = ?")?;
    let mut insert = c.prepare("INSERT INTO migrations (id, applied_at) VALUES (?, ?)")?;

    for file in Migrations::iter() {
        let id = file.as_ref();

        let result = (|| {
            select.bind(id)?;

            if let Some(applied_at) = select.next::<String>()? {
                tracing::debug!(id, applied_at, "migration already applied");
                return Ok(());
            }

            let Some(asset) = Migrations::get(id) else {
                anyhow::bail!("migration file not found: {id}");
            };

            let sql = std::str::from_utf8(asset.data.as_ref())
                .with_context(|| format!("migration {id} is not valid UTF-8"))?;

            c.execute(sql)
                .with_context(|| format!("executing migration {id}"))?;

            let now = Timestamp::now().to_string();
            insert.bind((id, now.as_str()))?;
            ensure!(insert.step()?.is_done(), "stepping migration insert");
            tracing::info!(id, "migration applied");
            Ok(())
        })();

        result.with_context(|| format!("migration {id}"))?;
    }

    Ok(())
}

fn ensure_mode(c: &sqll::Connection, mode: OpenMode) -> Result<()> {
    match mode {
        OpenMode::Normal => {
            let journal = c
                .prepare("PRAGMA journal_mode")?
                .into_iter::<String>()
                .next()
                .transpose()?;
            if journal.as_deref() != Some("delete") {
                tracing::warn!(?journal, "switching journal mode to delete");
                c.execute("PRAGMA journal_mode = delete;")?;
            }

            let synchronous = c
                .prepare("PRAGMA synchronous")?
                .into_iter::<i64>()
                .next()
                .transpose()?;
            if synchronous != Some(2) {
                tracing::warn!(?synchronous, "switching synchronous to full");
                c.execute("PRAGMA synchronous = full;")?;
            }
        }
        OpenMode::Bulk => {
            c.execute("PRAGMA journal_mode = off; PRAGMA synchronous = off;")?;
        }
    }

    Ok(())
}
