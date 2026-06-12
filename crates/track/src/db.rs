#![allow(clippy::too_many_arguments)]

use core::cell::UnsafeCell;
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::str;

use core::sync::atomic::{AtomicU64, Ordering};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use std::collections::{HashMap, HashSet};

use api::{
    Config, Date, EpisodeId, Image, ImageId, ImageKind, ImageSource, MarkTime, MovieId,
    MovieReleaseId, PendingId, ReleaseType, RemoteId, SeasonId, SeasonNumber, SeriesId, SyncSource,
    ThemeType, Timestamp, WatchedId, WatchedKind,
};
use rust_embed::RustEmbed;
use sqll::{OpenOptions, Row, SendStatement};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::spawn_blocking;

const MIGRATIONS_INIT: &str = r#"
CREATE TABLE IF NOT EXISTS migrations (
    id         TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL
);
"#;

struct Stmt {
    inner: SendStatement,
}

impl Stmt {
    fn new(inner: SendStatement) -> Self {
        Self { inner }
    }

    fn execute(&mut self, bind: impl sqll::Bind) -> Result<()> {
        self.inner.reset()?;
        self.inner.bind(bind)?;
        while !self.inner.step()?.is_done() {}
        self.inner.reset()?;
        Ok(())
    }

    fn query(&mut self) -> Result<BoundStmt<'_>> {
        self.inner.reset()?;
        Ok(BoundStmt { stmt: self })
    }

    fn bind(&mut self, bind: impl sqll::Bind) -> Result<BoundStmt<'_>> {
        self.inner.reset()?;
        self.inner.bind(bind)?;
        Ok(BoundStmt { stmt: self })
    }
}

struct BoundStmt<'stmt> {
    stmt: &'stmt mut Stmt,
}

impl<'stmt> BoundStmt<'stmt> {
    fn first<T>(self) -> Result<Option<T>>
    where
        T: for<'a> sqll::Row<'a>,
    {
        let value = self.stmt.inner.next()?;
        let mut this = ManuallyDrop::new(self);
        this.stmt.inner.reset()?;
        Ok(value)
    }

    #[inline]
    fn next<'this, T>(&'this mut self) -> Result<Option<T>>
    where
        T: sqll::Row<'this>,
    {
        Ok(self.stmt.inner.next()?)
    }

    /// Consume the bound statement and reset the underlying statement,
    /// surfacing any error. Unlike `Drop` (which resets silently), this lets a
    /// caller release the statement between sequential uses without a scope and
    /// still propagate a reset failure.
    fn reset(self) -> Result<()> {
        // Skip the `Drop` reset: we reset here and surface the error instead.
        let mut this = ManuallyDrop::new(self);
        this.stmt.inner.reset()?;
        Ok(())
    }
}

impl Drop for BoundStmt<'_> {
    fn drop(&mut self) {
        // Ensure the statement is reset when the bound statement goes out of
        // scope, so that the underlying statement can be reused.
        let _ = self.stmt.inner.reset();
    }
}

#[derive(RustEmbed)]
#[folder = "migrations"]
struct Migrations;

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
struct ImageRow {
    id: ImageId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
}

#[derive(Row)]
struct ImageMetaRow {
    kind: ImageKind,
    series_id: Option<SeriesId>,
    movie_id: Option<MovieId>,
}

#[derive(Row)]
struct SeriesImageRow {
    id: ImageId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    series_id: SeriesId,
}

#[derive(Row)]
struct MovieImageRow {
    id: ImageId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    movie_id: MovieId,
}

#[derive(Row)]
struct ImageSelectionRow {
    kind: ImageKind,
    source: ImageSource,
    path: String,
    width: u32,
    height: u32,
}

#[derive(Row)]
struct EpisodeScreenshotRow {
    episode_id: EpisodeId,
    source: ImageSource,
    path: String,
    width: u32,
    height: u32,
}

#[derive(Row)]
struct AllSeriesImageSelectionRow {
    series_id: SeriesId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    width: u32,
    height: u32,
}

#[derive(Row)]
struct AllMovieImageSelectionRow {
    movie_id: MovieId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    width: u32,
    height: u32,
}

#[derive(Row)]
struct SeasonRow {
    id: SeasonId,
    series_id: SeriesId,
    season: SeasonNumber,
    air_date: Option<Timestamp>,
    name: Option<String>,
    overview: Option<String>,
    watched_count: u32,
    total_count: u32,
}

#[derive(Row)]
struct EpisodeRow {
    id: EpisodeId,
    series_id: SeriesId,
    season: SeasonNumber,
    number: u32,
    absolute_number: Option<u32>,
    name: Option<String>,
    overview: Option<String>,
    aired: Option<Timestamp>,
    remote_id: Option<RemoteId>,
    pending: bool,
}

#[derive(Row)]
struct EpisodeIdRow {
    id: EpisodeId,
    season: SeasonNumber,
    number: u32,
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
    season: SeasonNumber,
    number: u32,
    episode_id: EpisodeId,
}

#[derive(Row)]
struct OrphanedWatchedRow {
    id: WatchedId,
    timestamp: Timestamp,
    series_id: SeriesId,
    season: SeasonNumber,
    episode: u32,
}

#[derive(Row)]
struct EpisodeNaturalKeyRow {
    series_id: SeriesId,
    season: SeasonNumber,
    number: u32,
}

#[derive(Row)]
struct UnwatchedEpisodeRow {
    id: EpisodeId,
    series_id: SeriesId,
    season: SeasonNumber,
    number: u32,
}

#[derive(Row)]
struct PendingBaseRow {
    episode_id: Option<api::EpisodeId>,
    movie_id: Option<api::MovieId>,
}

#[derive(Row)]
struct PendingEpisodeDetailRow {
    series_id: api::SeriesId,
    series_title: Option<String>,
    season: SeasonNumber,
    number: u32,
    episode_name: Option<String>,
    aired: Option<Timestamp>,
}

#[derive(Row)]
struct PendingMovieDetailRow {
    title: Option<String>,
    release_date: Option<Timestamp>,
}

#[derive(Row)]
struct PendingImageRow {
    source: ImageSource,
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
    season: SeasonNumber,
    number: u32,
    absolute_number: Option<u32>,
    name: Option<String>,
    overview: Option<String>,
    aired: Option<Timestamp>,
    remote_id: Option<RemoteId>,
}

macro_rules! statements {
    (
        $read_vis:vis struct $read_only:ident {
            $($read_name:ident: $read_sql:expr),* $(,)?
        }

        $write_vis:vis struct $write:ident {
            $($write_name:ident: $write_sql:expr),* $(,)?
        }
    ) => {
        $read_vis struct $read_only {
            $($read_name: Stmt,)*
        }

        impl $read_only {
            fn new(c: &sqll::Connection) -> Result<Self> {
                unsafe {
                    Ok(Self {
                        $(
                            $read_name: {
                                let stmt = c
                                    .prepare_with($read_sql)
                                    .persistent()
                                    .build()
                                    .context(concat!("Preparing statement ", stringify!($read_name)))?
                                    .into_send()?;

                                if !stmt.is_read_only() {
                                    return Err(anyhow!(concat!("Statement is not read-only: ", stringify!($read_name))));
                                }

                                Stmt::new(stmt)
                            },
                        )*
                    })
                }
            }
        }

        $write_vis struct $write {
            read_only: $read_only,
            $($write_name: Stmt,)*
        }

        impl $write {
            fn new(c: &sqll::Connection) -> Result<Self> {
                unsafe {
                    Ok(Self {
                        read_only: $read_only::new(c)?,
                        $(
                            $write_name: {
                                let stmt = c
                                    .prepare_with($write_sql)
                                    .persistent()
                                    .build()
                                    .context(concat!("Preparing statement ", stringify!($write_name)))?
                                    .into_send()?;

                                if stmt.is_read_only() {
                                    return Err(anyhow!(concat!("Write statement is read-only: ", stringify!($write_name))));
                                }

                                Stmt::new(stmt)
                            },
                        )*
                    })
                }
            }
        }

        impl ::core::ops::Deref for $write {
            type Target = $read_only;

            #[inline]
            fn deref(&self) -> &$read_only {
                &self.read_only
            }
        }

        impl ::core::ops::DerefMut for $write {
            #[inline]
            fn deref_mut(&mut self) -> &mut $read_only {
                &mut self.read_only
            }
        }
    }
}

statements! {
    struct InnerRead {
        // series
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

        // remotes (series and movies share one table)
        list_series_remotes: r#"
            SELECT remote_id FROM remotes WHERE series_id = ? ORDER BY rowid
        "#,
        list_all_series_remotes: r#"
            SELECT series_id, remote_id FROM remotes WHERE series_id IS NOT NULL ORDER BY series_id, rowid
        "#,
        series_id_by_remote: r#"
            SELECT series_id FROM remotes WHERE remote_id = ? LIMIT 1
        "#,

        // images (series and movies share one table)
        list_series_images: r#"
            SELECT id, kind, source, path FROM images
            WHERE series_id = ? ORDER BY kind, rank, id
        "#,
        list_all_series_images: r#"
            SELECT id, kind, source, path, series_id FROM images
            WHERE series_id IS NOT NULL ORDER BY series_id, kind, rank, id
        "#,
        list_season_episode_screenshots: r#"
            SELECT ei.episode_id, i.source, i.path, i.width, i.height
            FROM episode_images ei JOIN images i ON i.id = ei.image_id
            WHERE ei.kind = ? AND ei.episode_id IN (SELECT id FROM episodes WHERE series_id = ? AND season = ?)
        "#,
        list_movie_images: r#"
            SELECT id, kind, source, path FROM images
            WHERE movie_id = ? ORDER BY kind, rank, id
        "#,
        list_all_movie_images: r#"
            SELECT id, kind, source, path, movie_id FROM images
            WHERE movie_id IS NOT NULL ORDER BY movie_id, kind, rank, id
        "#,
        image_by_id: r#"
            SELECT kind, series_id, movie_id FROM images WHERE id = ?
        "#,

        // selection tables
        list_series_image_selections: r#"
            SELECT si.kind, i.source, i.path, i.width, i.height
            FROM series_images si JOIN images i ON i.id = si.image_id
            WHERE si.series_id = ?
        "#,
        list_all_series_image_selections: r#"
            SELECT si.series_id, si.kind, i.source, i.path, i.width, i.height
            FROM series_images si JOIN images i ON i.id = si.image_id
        "#,
        list_movie_image_selections: r#"
            SELECT mi.kind, i.source, i.path, i.width, i.height
            FROM movie_images mi JOIN images i ON i.id = mi.image_id
            WHERE mi.movie_id = ?
        "#,
        list_all_movie_image_selections: r#"
            SELECT mi.movie_id, mi.kind, i.source, i.path, i.width, i.height
            FROM movie_images mi JOIN images i ON i.id = mi.image_id
        "#,

        // seasons
        list_seasons: r#"
            SELECT s.id, s.series_id, s.season, s.air_date, s.name, s.overview,
                (SELECT COUNT(DISTINCT we.episode) FROM watched_episodes we WHERE we.series_id = s.series_id AND we.season = s.season) AS watched_count,
                (SELECT COUNT(*) FROM episodes e WHERE e.series_id = s.series_id AND e.season = s.season) AS total_count
            FROM seasons s WHERE s.series_id = ? ORDER BY s.season
        "#,
        episode_numbers_for_season: r#"
            SELECT episode FROM episodes WHERE series_id = ? AND season = ?
        "#,

        // episodes
        episode_natural_key: r#"
            SELECT series_id, season, episode FROM episodes WHERE id = ?
        "#,
        list_episode_ids_for_series: r#"
            SELECT id, season, episode FROM episodes WHERE series_id = ?
        "#,
        list_episodes: r#"
            SELECT e.id, e.series_id, e.season, e.episode, e.absolute_number, e.name, e.overview, e.aired, e.remote_id,
                   EXISTS(SELECT 1 FROM pending p WHERE p.episode_id = e.id) AS pending
            FROM episodes e
            WHERE e.series_id = ? AND e.season = ?
            ORDER BY e.episode
        "#,
        list_episodes_watched: r#"
            SELECT we.id, we.timestamp, we.season, we.episode, e.id AS episode_id
            FROM watched_episodes we
            JOIN episodes e ON e.series_id = we.series_id AND e.season = we.season AND e.episode = we.episode
            WHERE we.series_id = ?
            ORDER BY we.timestamp DESC
        "#,
        episode_aired_by_id: r#"
            SELECT aired FROM episodes WHERE id = ?
        "#,

        // movies
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
        list_movie_remotes: r#"
            SELECT remote_id FROM remotes WHERE movie_id = ? ORDER BY rowid
        "#,
        list_all_movie_remotes: r#"
            SELECT movie_id, remote_id FROM remotes WHERE movie_id IS NOT NULL ORDER BY movie_id, rowid
        "#,
        movie_id_by_remote: r#"
            SELECT movie_id FROM remotes WHERE remote_id = ? LIMIT 1
        "#,
        movie_released_by_id: r#"
            SELECT release_date FROM movies WHERE id = ?
        "#,

        // watched
        list_watched_by_episode: r#"
            SELECT we.id, we.timestamp, e.id AS episode_id, NULL AS movie_id, e.series_id
            FROM watched_episodes we
            JOIN episodes e ON e.series_id = we.series_id AND e.season = we.season AND e.episode = we.episode
            WHERE e.id = ?
            ORDER BY we.timestamp DESC
        "#,
        list_watched_by_movie: r#"
            SELECT id, timestamp, NULL AS episode_id, movie_id, NULL AS series_id
            FROM watched_movies WHERE movie_id = ? ORDER BY timestamp DESC
        "#,
        list_orphaned_for_series: r#"
            SELECT we.id, we.timestamp, we.series_id, we.season, we.episode
            FROM watched_episodes we
            LEFT JOIN episodes e
                ON e.series_id = we.series_id AND e.season = we.season AND e.episode = we.episode
            WHERE we.series_id = ? AND e.id IS NULL
            ORDER BY we.timestamp ASC
        "#,
        // select episodes which have 0 watched by series and season.
        select_unwatched_by_series_season: r#"
            SELECT id, series_id, season, episode FROM episodes
            WHERE series_id = ? AND season = ?
              AND NOT EXISTS (
                  SELECT 1 FROM watched_episodes we
                  WHERE we.series_id = episodes.series_id
                    AND we.season = episodes.season
                    AND we.episode = episodes.episode
              )
        "#,

        // pending table management
        has_pending_movie: r#"SELECT 1 FROM pending WHERE movie_id = ? LIMIT 1"#,
        has_pending_episode_for_series: r#"
            SELECT 1 FROM pending WHERE series_id = ? LIMIT 1
        "#,
        next_pending_episode_for_series: r#"
            SELECT e.id, e.aired
            FROM episodes e
            WHERE e.series_id = ?
              AND (e.aired IS NOT NULL AND e.aired <= ?)
              AND NOT EXISTS (
                  SELECT 1 FROM watched_episodes we
                  WHERE we.series_id = e.series_id AND we.season = e.season AND we.episode = e.episode
              )
            ORDER BY e.season, e.episode
            LIMIT 1
        "#,
        first_unwatched_episode_for_series: r#"
            SELECT e.id, e.aired
            FROM episodes e
            WHERE e.series_id = ?
              AND NOT EXISTS (
                  SELECT 1 FROM watched_episodes we
                  WHERE we.series_id = e.series_id AND we.season = e.season AND we.episode = e.episode
              )
            ORDER BY e.season, e.episode
            LIMIT 1
        "#,
        movies_needing_pending: r#"
            SELECT m.id, m.release_date
            FROM movies m
            WHERE m.tracked = 1
              AND (m.release_date IS NOT NULL AND m.release_date <= ?)
              AND NOT EXISTS (SELECT 1 FROM watched_movies wm WHERE wm.movie_id = m.id)
              AND NOT EXISTS (SELECT 1 FROM pending p WHERE p.movie_id = m.id)
        "#,
        list_pending_before: r#"
            SELECT episode_id, movie_id
            FROM pending
            WHERE timestamp <= ?
            ORDER BY timestamp DESC
        "#,
        pending_episode_detail: r#"
            SELECT e.series_id, s.title AS series_title, e.season, e.episode, e.name AS episode_name, e.aired
            FROM episodes e
            JOIN series s ON s.id = e.series_id
            WHERE e.id = ?
        "#,
        pending_movie_detail: r#"
            SELECT title, release_date FROM movies WHERE id = ?
        "#,
        image_for_series: r#"
            SELECT i.source, i.path
            FROM series_images si JOIN images i ON i.id = si.image_id
            WHERE si.series_id = ? AND si.kind = ?
        "#,
        image_for_movie: r#"
            SELECT i.source, i.path
            FROM movie_images mi JOIN images i ON i.id = mi.image_id
            WHERE mi.movie_id = ? AND mi.kind = ?
        "#,
        next_episode_after: r#"
            SELECT e.id, e.aired FROM episodes e
            JOIN episodes c ON c.id = ?2
            WHERE e.series_id = ?1
              AND (e.season > c.season OR (e.season = c.season AND e.episode > c.episode))
            ORDER BY e.season, e.episode
            LIMIT 1
        "#,

        // schedule: episodes airing in the next N days
        list_schedule: r#"
            SELECT e.series_id, s.title AS series_title,
                   e.id AS episode_id, e.season, e.episode, e.absolute_number,
                   e.name, e.overview, e.aired, e.remote_id
            FROM episodes e
            JOIN series s ON s.id = e.series_id
            WHERE s.tracked = 1
              AND e.aired > ?
              AND e.aired <= ?
            ORDER BY e.aired, s.title, e.season, e.episode
        "#,

        // all watched (for import dedup) — see list_all_watched_episodes / list_all_watched_movies

        // config
        get_config: r#"
            SELECT value FROM config WHERE key = ?
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
        list_movie_releases: r#"
            SELECT country, release_type, timestamp
            FROM movie_releases
            WHERE movie_id = ?
            ORDER BY timestamp, country, release_type
        "#,
        movie_release_by_type: r#"
            SELECT timestamp
            FROM movie_releases
            WHERE movie_id = ? AND release_type = ?
            ORDER BY timestamp
        "#,

        // digital-release pending discovery
        movies_needing_pending_digital: r#"
            SELECT m.id, MIN(mr.timestamp) AS release_timestamp
            FROM movies m
            JOIN movie_releases mr ON mr.movie_id = m.id AND mr.release_type = 'digital'
            WHERE m.tracked = 1
              AND mr.timestamp <= ?
              AND NOT EXISTS (SELECT 1 FROM watched_movies wm WHERE wm.movie_id = m.id)
              AND NOT EXISTS (SELECT 1 FROM pending p WHERE p.movie_id = m.id)
            GROUP BY m.id
        "#,
    }

    struct InnerWrite {
        // series
        insert_series: r#"
            INSERT INTO series (id, title, first_air, overview, tracked)
            VALUES (?, ?, ?, ?, ?)
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
        insert_series_remote: r#"
            INSERT OR IGNORE INTO remotes (series_id, remote_id) VALUES (?, ?)
        "#,
        delete_series_remote: r#"
            DELETE FROM remotes WHERE series_id = ? AND remote_id = ?
        "#,
        update_series_remote: r#"
            UPDATE remotes SET remote_id = ? WHERE series_id = ? AND remote_id = ?
        "#,

        // images (series and movies share one table)
        delete_series_images: r#"
            DELETE FROM images WHERE series_id = ?
        "#,
        insert_series_image: r#"
            INSERT INTO images (id, series_id, kind, source, path, width, height, rank) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(series_id, kind, path) WHERE series_id IS NOT NULL DO NOTHING
        "#,
        insert_episode_image: r#"
            INSERT INTO images (id, episode_id, kind, source, path, width, height) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(episode_id, kind, path) WHERE episode_id IS NOT NULL DO NOTHING
        "#,
        delete_episode_images_for_series: r#"
            DELETE FROM images WHERE episode_id IN (SELECT id FROM episodes WHERE series_id = ?)
        "#,
        delete_movie_images: r#"
            DELETE FROM images WHERE movie_id = ?
        "#,
        insert_movie_image: r#"
            INSERT INTO images (id, movie_id, kind, source, path, width, height, rank) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(movie_id, kind, path) WHERE movie_id IS NOT NULL DO NOTHING
        "#,

        // selection tables
        set_series_image_selection: r#"
            INSERT OR REPLACE INTO series_images (series_id, kind, image_id) VALUES (?, ?, ?)
        "#,
        delete_series_image_selection: r#"
            DELETE FROM series_images WHERE series_id = ? AND kind = ?
        "#,
        set_movie_image_selection: r#"
            INSERT OR REPLACE INTO movie_images (movie_id, kind, image_id) VALUES (?, ?, ?)
        "#,
        delete_movie_image_selection: r#"
            DELETE FROM movie_images WHERE movie_id = ? AND kind = ?
        "#,
        set_episode_image_selection: r#"
            INSERT OR REPLACE INTO episode_images (episode_id, kind, image_id) VALUES (?, ?, ?)
        "#,

        // seasons
        upsert_season: r#"
            INSERT INTO seasons (id, series_id, season, air_date, name, overview)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(series_id, season) DO UPDATE SET
                air_date  = excluded.air_date,
                name      = excluded.name,
                overview  = excluded.overview
        "#,
        delete_season: r#"DELETE FROM seasons WHERE series_id = ?1 AND season = ?2"#,
        delete_season_episodes: r#"DELETE FROM episodes WHERE series_id = ?1 AND season = ?2"#,
        delete_episode_by_place: r#"
            DELETE FROM episodes WHERE series_id = ? AND season = ? AND episode = ?
        "#,

        // episodes
        upsert_episode: r#"
            INSERT INTO episodes (id, series_id, season, episode, absolute_number, name, overview, aired, remote_id)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(series_id, season, episode) DO UPDATE SET
                absolute_number = excluded.absolute_number,
                name            = excluded.name,
                overview        = excluded.overview,
                aired           = excluded.aired,
                remote_id       = excluded.remote_id
        "#,
        update_episode_aired: r#"
            UPDATE episodes SET aired = ? WHERE series_id = ? AND season = ? AND episode = ?
        "#,

        // movies
        insert_movie: r#"
            INSERT INTO movies (id, title, release_date, overview, tracked)
            VALUES (?, ?, ?, ?, ?)
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
            SET title = ?, release_date = ?, overview = ?
            WHERE id = ?
        "#,
        delete_movie: r#"
            DELETE FROM movies WHERE id = ?
        "#,
        insert_movie_remote: r#"
            INSERT OR IGNORE INTO remotes (movie_id, remote_id) VALUES (?, ?)
        "#,
        delete_movie_remote: r#"
            DELETE FROM remotes WHERE movie_id = ? AND remote_id = ?
        "#,
        update_movie_remote: r#"
            UPDATE remotes SET remote_id = ? WHERE movie_id = ? AND remote_id = ?
        "#,

        // watched
        insert_watched_episode: r#"
            INSERT OR IGNORE INTO watched_episodes (id, timestamp, series_id, season, episode)
            VALUES (?, ?, ?, ?, ?)
        "#,
        insert_watched_movie: r#"
            INSERT OR IGNORE INTO watched_movies (id, timestamp, movie_id)
            VALUES (?, ?, ?)
        "#,
        delete_watched_episode: r#"
            DELETE FROM watched_episodes WHERE id = ?
        "#,
        delete_watched_movie: r#"
            DELETE FROM watched_movies WHERE id = ?
        "#,
        move_watched_episode: r#"
            UPDATE watched_episodes SET season = ?, episode = ? WHERE id = ?
        "#,

        // pending table management
        upsert_pending_episode: r#"
            INSERT INTO pending (id, timestamp, series_id, episode_id) VALUES (?, ?, ?, ?)
            ON CONFLICT(series_id) WHERE series_id IS NOT NULL
                DO UPDATE SET episode_id = excluded.episode_id, timestamp = excluded.timestamp
        "#,
        upsert_pending_movie: r#"
            INSERT INTO pending (id, timestamp, movie_id) VALUES (?, ?, ?)
            ON CONFLICT(movie_id) WHERE movie_id IS NOT NULL
                DO UPDATE SET timestamp = excluded.timestamp
        "#,
        delete_pending_episode: r#"DELETE FROM pending WHERE series_id = ?"#,
        delete_pending_movie: r#"DELETE FROM pending WHERE movie_id = ?"#,

        // config
        set_config: r#"
            INSERT INTO config (key, value) VALUES (?, ?)
            ON CONFLICT (key) DO UPDATE SET value = excluded.value
        "#,
        delete_config: r#"
            DELETE FROM config WHERE key = ?
        "#,

        // movie releases
        upsert_movie_release: r#"
            INSERT INTO movie_releases (id, movie_id, country, release_type, timestamp)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(movie_id, country, release_type)
                DO UPDATE SET timestamp = excluded.timestamp
        "#,

        // last_synced_at stamping
        set_series_synced_at: r#"
            UPDATE series SET last_synced_at = ? WHERE id = ?
        "#,
        set_movie_synced_at: r#"
            UPDATE movies SET last_synced_at = ? WHERE id = ?
        "#,
    }
}

impl InnerRead {
    fn get_config(&mut self, key: &str) -> Result<Option<String>> {
        self.get_config.bind((key,))?.first()
    }
}

impl InnerWrite {
    fn set_config(&mut self, key: &str, value: &str) -> Result<()> {
        self.set_config.execute((key, value))?;
        Ok(())
    }

    fn delete_config(&mut self, key: &str) -> Result<()> {
        self.delete_config.execute((key,))?;
        Ok(())
    }
}

impl InnerRead {
    fn episode_mark_time(
        &mut self,
        episode: EpisodeId,
        mark_time: MarkTime,
        now: Timestamp,
    ) -> Result<Timestamp> {
        match mark_time {
            MarkTime::Now => Ok(now),
            MarkTime::WhenAired => {
                let Some(aired) = self
                    .episode_aired_by_id
                    .bind((episode,))?
                    .first()?
                    .flatten()
                else {
                    anyhow::bail!("Episode has no air date");
                };

                Ok(aired)
            }
        }
    }

    fn image_for_series(
        &mut self,
        series_id: SeriesId,
        kind: ImageKind,
    ) -> Result<Option<api::Image>> {
        let poster_row = self
            .image_for_series
            .bind((series_id, kind))?
            .first::<PendingImageRow>()?;
        Ok(poster_row.map(|p| api::Image::new(p.source, &p.path)))
    }

    fn image_for_movie(
        &mut self,
        movie_id: MovieId,
        kind: ImageKind,
    ) -> Result<Option<api::Image>> {
        let poster_row = self
            .image_for_movie
            .bind((movie_id, kind))?
            .first::<PendingImageRow>()?;
        Ok(poster_row.map(|p| api::Image::new(p.source, &p.path)))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum OpenMode {
    /// Full synchronization — safe for the server.
    Normal,
    /// No journaling or fsync — fast for bulk import; not crash-safe.
    Bulk,
}

struct State {
    inner: Box<[UnsafeCell<InnerRead>]>,
    // Bit set of which inner connections are in use.
    used: AtomicU64,
    write: UnsafeCell<InnerWrite>,
    wr: usize,
    s: Arc<Semaphore>,
}

unsafe impl Send for State {}
unsafe impl Sync for State {}

struct SharedState {
    read: NonNull<InnerRead>,
    state: Arc<State>,
    index: usize,
    _permit: OwnedSemaphorePermit,
}

impl Deref for SharedState {
    type Target = InnerRead;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.read.as_ref() }
    }
}

impl DerefMut for SharedState {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.read.as_mut() }
    }
}

unsafe impl Send for SharedState {}

impl Drop for SharedState {
    fn drop(&mut self) {
        // Mark the connection as unused again. We can do this without locking
        // because each permit corresponds to one bit in the used set.
        self.state
            .used
            .fetch_and(!(1 << self.index), Ordering::AcqRel);
    }
}

struct ExclusiveState {
    write: NonNull<InnerWrite>,
    _state: Arc<State>,
    _permit: OwnedSemaphorePermit,
}

impl Deref for ExclusiveState {
    type Target = InnerWrite;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.write.as_ref() }
    }
}

impl DerefMut for ExclusiveState {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.write.as_mut() }
    }
}

unsafe impl Send for ExclusiveState {}

impl State {
    async fn shared(self: Arc<Self>) -> SharedState {
        let permit = self
            .s
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");

        // Find the first unused connection in the used set, mark it as used,
        // and return it. We can do this without locking because each permit
        // corresponds to one bit in the used set.
        let index = loop {
            let value = self.used.load(Ordering::Acquire);
            let index = value.trailing_ones();

            if self
                .used
                .compare_exchange(
                    value,
                    value | (1 << index),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                break index as usize;
            }
        };

        SharedState {
            read: unsafe {
                let slice = self.inner[index].get();
                NonNull::new_unchecked(&mut *slice)
            },
            state: self,
            index,
            _permit: permit,
        }
    }

    async fn exclusive(self: Arc<Self>) -> ExclusiveState {
        let permit = self
            .s
            .clone()
            .acquire_many_owned(self.wr as u32)
            .await
            .expect("semaphore closed");

        ExclusiveState {
            write: unsafe { NonNull::new_unchecked(&mut *self.write.get()) },
            _state: self,
            _permit: permit,
        }
    }
}

pub(crate) struct Database {
    inner: Arc<State>,
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Database {
    pub(crate) fn open(
        path: impl AsRef<Path>,
        mode: OpenMode,
        read_concurrency: usize,
    ) -> Result<Self> {
        anyhow::ensure!(
            read_concurrency > 0 && read_concurrency <= 64,
            "read_concurrency must be between 1 and 64"
        );

        let path = path.as_ref();

        let mut read = Vec::with_capacity(read_concurrency);

        let c = OpenOptions::new()
            .extended_result_codes()
            .read_write()
            .create()
            .no_mutex()
            .open(path.as_os_str())
            .with_context(|| anyhow!("Opening database at {}", path.display()))?;

        do_migrations(&c).context("Running migrations")?;

        ensure_mode(&c, mode).context("Setting database mode")?;

        let write = UnsafeCell::new(InnerWrite::new(&c).context("Preparing statements")?);

        for _ in 0..read_concurrency {
            let c = OpenOptions::new()
                .extended_result_codes()
                .read_only()
                .no_mutex()
                .open(path.as_os_str())
                .with_context(|| anyhow!("Opening database at {}", path.display()))?;

            ensure_mode(&c, mode).context("Setting database mode")?;

            let inner = InnerRead::new(&c).context("Preparing statements")?;
            read.push(UnsafeCell::new(inner));
        }

        Ok(Self {
            inner: Arc::new(State {
                inner: Box::from(read),
                write,
                used: AtomicU64::new(0),
                wr: read_concurrency,
                s: Arc::new(Semaphore::new(read_concurrency)),
            }),
        })
    }

    pub(crate) async fn create_series(
        &self,
        id: SeriesId,
        title: &str,
        first_air: Option<Timestamp>,
        overview: &str,
    ) -> Result<()> {
        let title = title.to_owned();
        let overview = overview.to_owned();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.insert_series
                .execute((id, &title[..], first_air.as_ref(), &overview[..], true))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn series_id_by_remote(
        &self,
        remote_id: &RemoteId,
    ) -> Result<Option<SeriesId>> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            s.series_id_by_remote
                .bind((&remote_id,))?
                .first::<SeriesId>()
        });

        result.await?
    }

    pub(crate) async fn add_series_remote(
        &self,
        series_id: SeriesId,
        remote_id: &RemoteId,
    ) -> Result<()> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.insert_series_remote.execute((series_id, &remote_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn remove_series_remote(
        &self,
        series_id: SeriesId,
        remote_id: &RemoteId,
    ) -> Result<()> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.delete_series_remote.execute((series_id, &remote_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn update_series_remote(
        &self,
        series_id: SeriesId,
        old: &RemoteId,
        new: &RemoteId,
    ) -> Result<()> {
        let old = old.clone();
        let new = new.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.update_series_remote.execute((&new, series_id, &old))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn series(&self) -> Result<Vec<api::Series>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out: Vec<api::Series> = Vec::new();
            let mut id_to_idx: HashMap<SeriesId, usize> = HashMap::new();

            let mut stmt = s.list_series.query()?;

            while let Some(row) = stmt.next::<SeriesRow>()? {
                let idx = out.len();
                id_to_idx.insert(row.id, idx);

                out.push(series_from_row(row));
            }

            stmt.reset()?;

            for series in &mut out {
                series.poster = s.image_for_series(series.id, ImageKind::Poster)?;
                series.banner = s.image_for_series(series.id, ImageKind::Banner)?;
            }

            let mut stmt = s.list_all_series_remotes.query()?;

            while let Some((series_id, remote_id)) = stmt.next::<(SeriesId, RemoteId)>()? {
                if let Some(o) = id_to_idx.get(&series_id).and_then(|&i| out.get_mut(i)) {
                    o.remotes.push(remote_id);
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_series_images.query()?;

            while let Some(r) = stmt.next::<SeriesImageRow>()? {
                if let Some(o) = id_to_idx.get(&r.series_id).and_then(|&i| out.get_mut(i)) {
                    o.images.push(series_image_from_row(r));
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_series_image_selections.query()?;

            while let Some(row) = stmt.next::<AllSeriesImageSelectionRow>()? {
                if let Some(o) = id_to_idx.get(&row.series_id).and_then(|&i| out.get_mut(i)) {
                    apply_image_selection(
                        o,
                        ImageSelectionRow {
                            kind: row.kind,
                            source: row.source,
                            path: row.path,
                            width: row.width,
                            height: row.height,
                        },
                    );
                }
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn series_by_id(&self, id: SeriesId) -> Result<Option<api::Series>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let Some(r) = s.series_by_id.bind((id,))?.first::<SeriesRow>()? else {
                return Ok(None);
            };

            let mut series = series_from_row(r);

            let mut stmt = s.list_series_remotes.bind((id,))?;

            while let Some(remote_id) = stmt.next::<RemoteId>()? {
                series.remotes.push(remote_id);
            }

            stmt.reset()?;

            let mut stmt = s.list_series_images.bind((id,))?;

            while let Some(row) = stmt.next::<ImageRow>()? {
                series.images.push(image_from_row(row));
            }

            stmt.reset()?;

            let mut stmt = s.list_series_image_selections.bind((id,))?;

            while let Some(sel) = stmt.next::<ImageSelectionRow>()? {
                apply_image_selection(&mut series, sel);
            }

            stmt.reset()?;

            series.poster = s.image_for_series(series.id, ImageKind::Poster)?;
            series.banner = s.image_for_series(series.id, ImageKind::Banner)?;
            Ok(Some(series))
        });

        result.await?
    }

    pub(crate) async fn update_series(
        &self,
        id: SeriesId,
        title: Option<&str>,
        first_air: Option<Timestamp>,
        overview: Option<&str>,
        tracked: bool,
    ) -> Result<()> {
        let title = title.map(str::to_owned);
        let overview = overview.map(str::to_owned);

        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.update_series.execute((
                title.as_deref(),
                first_air.as_ref(),
                overview.as_deref(),
                tracked,
                id,
            ))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn delete_series(&self, id: SeriesId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.delete_series.execute((id,))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_series_tracked(&self, id: SeriesId, tracked: bool) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_series_tracked.execute((tracked, id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_series_sync_source(
        &self,
        id: SeriesId,
        source: SyncSource,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_series_sync_source.execute((source, id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_series_language(
        &self,
        id: SeriesId,
        language: Option<String>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_series_language.execute((language.as_deref(), id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn upsert_season(
        &self,
        series_id: SeriesId,
        number: SeasonNumber,
        air_date: Option<Timestamp>,
        name: Option<&str>,
        overview: Option<&str>,
    ) -> Result<()> {
        let name = name.map(str::to_owned);
        let overview = overview.map(str::to_owned);
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.upsert_season.execute((
                SeasonId::random(),
                series_id,
                number,
                air_date.as_ref(),
                name.as_deref(),
                overview.as_deref(),
            ))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn seasons(&self, series_id: SeriesId) -> Result<Vec<api::Season>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_seasons.bind((series_id,))?;

            while let Some(r) = stmt.next::<SeasonRow>()? {
                out.push(season_from_row(r));
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn prune_seasons(
        &self,
        series_id: SeriesId,
        kept: &HashSet<SeasonNumber>,
    ) -> Result<Vec<SeasonNumber>> {
        let existing = self.seasons(series_id).await?;
        let mut removed = Vec::new();

        for season in existing {
            if kept.contains(&season.season) {
                continue;
            }

            let n = season.season;
            let mut s = self.inner.clone().exclusive().await;

            let result = spawn_blocking(move || {
                s.delete_season_episodes.execute((series_id, n))?;
                s.delete_season.execute((series_id, n))?;
                Ok::<_, anyhow::Error>(())
            });

            result.await??;
            removed.push(season.season);
        }

        Ok(removed)
    }

    pub(crate) async fn prune_season_episodes(
        &self,
        series_id: SeriesId,
        season: SeasonNumber,
        kept: &HashSet<u32>,
    ) -> Result<()> {
        let kept = kept.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            let mut to_delete = Vec::new();

            {
                let mut stmt = s.episode_numbers_for_season.bind((series_id, season))?;

                while let Some(number) = stmt.next::<u32>()? {
                    if !kept.contains(&number) {
                        to_delete.push(number);
                    }
                }
            }

            for number in to_delete {
                s.delete_episode_by_place
                    .execute((series_id, season, number))?;
            }

            Ok(())
        });

        result.await?
    }

    pub(crate) async fn upsert_episode(
        &self,
        id: EpisodeId,
        series_id: SeriesId,
        season: SeasonNumber,
        number: u32,
        absolute_number: Option<u32>,
        name: Option<&str>,
        overview: Option<&str>,
        aired: Option<Timestamp>,
        remote_id: Option<&RemoteId>,
    ) -> Result<()> {
        let name = name.map(str::to_owned);
        let overview = overview.map(str::to_owned);
        let remote_id = remote_id.cloned();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.upsert_episode.execute((
                id,
                series_id,
                season,
                number,
                absolute_number,
                name.as_deref(),
                overview.as_deref(),
                aired.as_ref(),
                remote_id.as_ref(),
            ))?;
            Ok(())
        });

        result.await?
    }

    /// Map of `(season, number)` to the existing episode id for a series, so a
    /// re-sync can reuse stable ids rather than allocating new ones.
    pub(crate) async fn episode_ids(
        &self,
        series_id: SeriesId,
    ) -> Result<HashMap<(SeasonNumber, u32), EpisodeId>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = HashMap::new();

            let mut stmt = s.list_episode_ids_for_series.bind((series_id,))?;

            while let Some(r) = stmt.next::<EpisodeIdRow>()? {
                out.insert((r.season, r.number), r.id);
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn episodes(
        &self,
        series_id: SeriesId,
        season: SeasonNumber,
    ) -> Result<Vec<api::Episode>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut idx_by_id = HashMap::new();

            let mut stmt = s.list_episodes.bind((series_id, season))?;

            while let Some(r) = stmt.next::<EpisodeRow>()? {
                idx_by_id.insert(r.id, out.len());
                out.push(episode_from_row(r));
            }

            stmt.reset()?;

            let mut stmt = s.list_season_episode_screenshots.bind((
                ImageKind::Screenshot,
                series_id,
                season,
            ))?;

            while let Some(r) = stmt.next::<EpisodeScreenshotRow>()? {
                if let Some(&i) = idx_by_id.get(&r.episode_id) {
                    out[i].screenshot =
                        Some(Image::new_with_dims(r.source, &r.path, r.width, r.height));
                }
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn mark_watched_remaining(
        &self,
        series_id: SeriesId,
        season: SeasonNumber,
        mark_time: MarkTime,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            let mut unwatched = Vec::new();

            let mut stmt = s
                .select_unwatched_by_series_season
                .bind((series_id, season))?;

            while let Some(r) = stmt.next::<UnwatchedEpisodeRow>()? {
                unwatched.push(r);
            }

            stmt.reset()?;

            for r in unwatched {
                let timestamp = s.episode_mark_time(r.id, mark_time, now)?;
                s.insert_watched_episode.execute((
                    WatchedId::random(),
                    timestamp,
                    r.series_id,
                    r.season,
                    r.number,
                ))?;
            }

            Ok(())
        });

        result.await?
    }

    pub(crate) async fn episodes_watched(
        &self,
        series_id: SeriesId,
    ) -> Result<Vec<api::WatchedEpisode>> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_episodes_watched.bind((series_id,))?;

            while let Some(r) = stmt.next::<WatchedEpisodeRow>()? {
                out.push(watched_episode_from_row(r));
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn episode_aired_by_id(&self, id: EpisodeId) -> Result<Option<Timestamp>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let stmt = s.episode_aired_by_id.bind((id,))?;
            Ok(stmt.first::<Option<Timestamp>>()?.flatten())
        });

        result.await?
    }

    pub(crate) async fn update_episodes_aired(
        &self,
        series_id: SeriesId,
        updates: Vec<(SeasonNumber, u32, Timestamp)>,
    ) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            for &(season, number, aired) in &updates {
                s.update_episode_aired
                    .execute((aired, series_id, season, number))?;
            }
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn create_movie(
        &self,
        id: MovieId,
        title: &str,
        release_date: Option<Timestamp>,
        overview: &str,
        tracked: bool,
    ) -> Result<()> {
        let title = title.to_owned();
        let overview = overview.to_owned();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.insert_movie
                .execute((id, &title[..], release_date, &overview[..], tracked))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn movie_id_by_remote(&self, remote_id: &RemoteId) -> Result<Option<MovieId>> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            Ok(s.movie_id_by_remote
                .bind((&remote_id,))?
                .first::<Option<MovieId>>()?
                .flatten())
        });

        result.await?
    }

    pub(crate) async fn add_movie_remote(
        &self,
        movie_id: MovieId,
        remote_id: &RemoteId,
    ) -> Result<()> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.insert_movie_remote.execute((movie_id, &remote_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn remove_movie_remote(
        &self,
        movie_id: MovieId,
        remote_id: &RemoteId,
    ) -> Result<()> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.delete_movie_remote.execute((movie_id, &remote_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn update_movie_remote(
        &self,
        movie_id: MovieId,
        old: &RemoteId,
        new: &RemoteId,
    ) -> Result<()> {
        let old = old.clone();
        let new = new.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.update_movie_remote.execute((&new, movie_id, &old))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn movies(&self) -> Result<Vec<api::Movie>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out: Vec<api::Movie> = Vec::new();
            let mut id_to_idx: HashMap<MovieId, usize> = HashMap::new();

            let mut stmt = s.list_movies.query()?;

            while let Some(row) = stmt.next::<MovieRow>()? {
                let index = out.len();
                id_to_idx.insert(row.id, index);

                out.push(movie_from_row(row));
            }

            stmt.reset()?;

            for movie in &mut out {
                movie.banner = s.image_for_movie(movie.id, ImageKind::Banner)?;
                movie.poster = s.image_for_movie(movie.id, ImageKind::Poster)?;
            }

            let mut stmt = s.list_all_movie_remotes.query()?;

            while let Some((movie_id, remote_id)) = stmt.next::<(MovieId, RemoteId)>()? {
                if let Some(&index) = id_to_idx.get(&movie_id)
                    && let Some(o) = out.get_mut(index)
                {
                    o.remotes.push(remote_id);
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_movie_images.query()?;

            while let Some(r) = stmt.next::<MovieImageRow>()? {
                if let Some(o) = id_to_idx.get(&r.movie_id).and_then(|&i| out.get_mut(i)) {
                    o.images.push(movie_image_from_row(r));
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_movie_image_selections.query()?;

            while let Some(row) = stmt.next::<AllMovieImageSelectionRow>()? {
                if let Some(o) = id_to_idx.get(&row.movie_id).and_then(|&i| out.get_mut(i)) {
                    apply_movie_image_selection(
                        o,
                        ImageSelectionRow {
                            kind: row.kind,
                            source: row.source,
                            path: row.path,
                            width: row.width,
                            height: row.height,
                        },
                    );
                }
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn movie_by_id(&self, id: MovieId) -> Result<Option<api::Movie>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let Some(r) = s.movie_by_id.bind((id,))?.first::<MovieRow>()? else {
                return Ok(None);
            };

            let movie_id = r.id;
            let mut movie = movie_from_row(r);

            let mut stmt = s.list_movie_remotes.bind((movie_id,))?;

            while let Some(remote_id) = stmt.next::<RemoteId>()? {
                movie.remotes.push(remote_id);
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_images.bind((movie_id,))?;

            while let Some(r) = stmt.next::<ImageRow>()? {
                movie.images.push(image_from_row(r));
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_image_selections.bind((movie_id,))?;

            while let Some(sel) = stmt.next::<ImageSelectionRow>()? {
                apply_movie_image_selection(&mut movie, sel);
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_releases.bind((movie_id,))?;

            while let Some(r) = stmt.next::<MovieReleaseRow>()? {
                movie.releases.push(api::MovieRelease {
                    country: r.country,
                    release_type: r.release_type,
                    timestamp: r.timestamp,
                });
            }

            stmt.reset()?;

            movie.pending = s
                .has_pending_movie
                .bind((movie_id,))?
                .first::<(i64,)>()?
                .is_some();

            movie.poster = s.image_for_movie(movie_id, ImageKind::Poster)?;
            movie.banner = s.image_for_movie(movie_id, ImageKind::Banner)?;

            Ok(Some(movie))
        });

        result.await?
    }

    pub(crate) async fn movie_release_by_type(
        &self,
        id: MovieId,
        ty: ReleaseType,
    ) -> Result<Option<Timestamp>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let stmt = s.movie_release_by_type.bind((id, ty))?;

            let Some(timestamp) = stmt.first::<Timestamp>()? else {
                return Ok(None);
            };

            Ok(Some(timestamp))
        });

        result.await?
    }

    pub(crate) async fn series_by_remote_id(
        &self,
        remote_id: &RemoteId,
    ) -> Result<Option<api::Series>> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let Some(row) = s
                .series_by_remote
                .bind((remote_id,))?
                .first::<SeriesRow>()?
            else {
                return Ok(None);
            };

            let series_id = row.id;
            let mut series = series_from_row(row);

            let mut stmt = s.list_series_remotes.bind((series_id,))?;

            while let Some(remote_id) = stmt.next::<RemoteId>()? {
                series.remotes.push(remote_id);
            }

            stmt.reset()?;

            let mut stmt = s.list_series_images.bind((series_id,))?;

            while let Some(r) = stmt.next::<ImageRow>()? {
                series.images.push(image_from_row(r));
            }

            stmt.reset()?;

            let mut stmt = s.list_series_image_selections.bind((series_id,))?;

            while let Some(sel) = stmt.next::<ImageSelectionRow>()? {
                apply_image_selection(&mut series, sel);
            }

            stmt.reset()?;

            series.poster = s.image_for_series(series_id, ImageKind::Poster)?;
            series.banner = s.image_for_series(series_id, ImageKind::Banner)?;
            Ok(Some(series))
        });

        result.await?
    }

    pub(crate) async fn movie_by_remote_id(
        &self,
        remote_id: &RemoteId,
    ) -> Result<Option<api::Movie>> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let Some(row) = s.movie_by_remote.bind((remote_id,))?.first::<MovieRow>()? else {
                return Ok(None);
            };

            let movie_id = row.id;
            let mut movie = movie_from_row(row);

            let mut stmt = s.list_movie_remotes.bind((movie_id,))?;

            while let Some(remote_id) = stmt.next::<RemoteId>()? {
                movie.remotes.push(remote_id);
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_images.bind((movie_id,))?;

            while let Some(r) = stmt.next::<ImageRow>()? {
                movie.images.push(image_from_row(r));
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_image_selections.bind((movie_id,))?;

            while let Some(sel) = stmt.next::<ImageSelectionRow>()? {
                apply_movie_image_selection(&mut movie, sel);
            }

            stmt.reset()?;

            movie.poster = s.image_for_movie(movie_id, ImageKind::Poster)?;
            movie.banner = s.image_for_movie(movie_id, ImageKind::Banner)?;

            Ok(Some(movie))
        });

        result.await?
    }

    pub(crate) async fn update_movie(
        &self,
        id: MovieId,
        title: Option<&str>,
        release_date: Option<Timestamp>,
        overview: Option<&str>,
    ) -> Result<()> {
        let title = title.map(str::to_owned);
        let overview = overview.map(str::to_owned);
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.update_movie.execute((
                title.as_deref(),
                release_date.as_ref(),
                overview.as_deref(),
                id,
            ))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn delete_movie(&self, id: MovieId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.delete_movie.execute((id,))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_movie_tracked(&self, id: MovieId, tracked: bool) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_movie_tracked.execute((tracked, id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_movie_sync_source(
        &self,
        id: MovieId,
        source: SyncSource,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_movie_sync_source.execute((source, id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_movie_language(
        &self,
        id: MovieId,
        language: Option<String>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_movie_language.execute((language.as_deref(), id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn clear_series_images(&self, series_id: SeriesId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;
        spawn_blocking(move || {
            s.delete_series_images.execute((series_id,))?;
            Ok(())
        })
        .await?
    }

    pub(crate) async fn clear_movie_images(&self, movie_id: MovieId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;
        spawn_blocking(move || {
            s.delete_movie_images.execute((movie_id,))?;
            Ok(())
        })
        .await?
    }

    pub(crate) async fn clear_episode_images(&self, series_id: SeriesId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;
        spawn_blocking(move || {
            s.delete_episode_images_for_series.execute((series_id,))?;
            Ok(())
        })
        .await?
    }

    pub(crate) async fn upsert_episode_image(
        &self,
        id: ImageId,
        episode_id: EpisodeId,
        kind: ImageKind,
        image: &Image,
    ) -> Result<()> {
        let image = image.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.insert_episode_image.execute((
                id,
                episode_id,
                kind,
                image.key().source(),
                image.key().path(),
                image.width(),
                image.height(),
            ))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_episode_image_selection(
        &self,
        episode_id: EpisodeId,
        kind: ImageKind,
        image_id: ImageId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_episode_image_selection
                .execute((episode_id, kind, image_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn upsert_series_image(
        &self,
        id: ImageId,
        series_id: SeriesId,
        kind: ImageKind,
        rank: u32,
        image: &Image,
    ) -> Result<()> {
        let image = image.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.insert_series_image.execute((
                id,
                series_id,
                kind,
                image.key().source(),
                image.key().path(),
                image.width(),
                image.height(),
                rank,
            ))?;

            Ok(())
        });

        result.await?
    }

    pub(crate) async fn upsert_movie_image(
        &self,
        id: ImageId,
        movie_id: MovieId,
        kind: ImageKind,
        rank: u32,
        image: &Image,
    ) -> Result<()> {
        let image = image.clone();
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.insert_movie_image.execute((
                id,
                movie_id,
                kind,
                image.key().source(),
                image.key().path(),
                image.width(),
                image.height(),
                rank,
            ))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_series_image_selection(
        &self,
        series_id: SeriesId,
        kind: ImageKind,
        image_id: ImageId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_series_image_selection
                .execute((series_id, kind, image_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_movie_image_selection(
        &self,
        movie_id: MovieId,
        kind: ImageKind,
        image_id: ImageId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_movie_image_selection
                .execute((movie_id, kind, image_id))?;
            Ok(())
        });

        result.await?
    }

    /// Selects the given image for its owning entity + kind, replacing any
    /// prior selection. Returns which entity owns the image.
    pub(crate) async fn select_image(&self, id: ImageId) -> Result<api::ImageOwner> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            let row = s
                .image_by_id
                .bind((id,))?
                .first::<ImageMetaRow>()?
                .context("Expected image to exist")?;

            let kind = row.kind;

            let owner = match (row.series_id, row.movie_id) {
                (Some(series_id), _) => {
                    s.set_series_image_selection
                        .execute((series_id, kind, id))?;

                    api::ImageOwner::Series(series_id)
                }
                (_, Some(movie_id)) => {
                    s.set_movie_image_selection.execute((movie_id, kind, id))?;

                    api::ImageOwner::Movie(movie_id)
                }
                _ => anyhow::bail!("Image has no owner"),
            };

            Ok(owner)
        });

        result.await?
    }

    pub(crate) async fn clear_selected_image(
        &self,
        owner: api::ImageOwner,
        kind: ImageKind,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            match owner {
                api::ImageOwner::Series(series_id) => {
                    s.delete_series_image_selection.execute((series_id, kind))?;
                }
                api::ImageOwner::Movie(movie_id) => {
                    s.delete_movie_image_selection.execute((movie_id, kind))?;
                }
            }

            Ok(())
        });

        result.await?
    }

    pub(crate) async fn mark_watched(
        &self,
        id: WatchedId,
        kind: WatchedKind,
        mark_time: MarkTime,
        now: Timestamp,
    ) -> Result<api::Watched> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            let (watched_id, timestamp) = match kind {
                WatchedKind::Episode { episode, .. } => {
                    let timestamp = s.episode_mark_time(episode, mark_time, now)?;

                    let key = s
                        .episode_natural_key
                        .bind((episode,))?
                        .first::<EpisodeNaturalKeyRow>()?
                        .context("Expected episode to exist")?;

                    s.insert_watched_episode.execute((
                        id,
                        timestamp,
                        key.series_id,
                        key.season,
                        key.number,
                    ))?;
                    (id, timestamp)
                }
                WatchedKind::Movie { movie } => {
                    let timestamp = match mark_time {
                        MarkTime::Now => now,
                        MarkTime::WhenAired => s
                            .movie_released_by_id
                            .bind((movie,))?
                            .first::<Option<Timestamp>>()?
                            .flatten()
                            .context("Movie has no release date")?,
                    };

                    s.insert_watched_movie.execute((id, timestamp, movie))?;
                    (id, timestamp)
                }
            };

            Ok(api::Watched {
                id: watched_id,
                timestamp,
                kind,
            })
        });

        result.await?
    }

    pub(crate) async fn insert_watched_episode(
        &self,
        id: WatchedId,
        timestamp: Timestamp,
        series_id: SeriesId,
        season: api::SeasonNumber,
        episode: u32,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.insert_watched_episode
                .execute((id, timestamp, series_id, season, episode))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn insert_watched_movie(
        &self,
        id: WatchedId,
        timestamp: Timestamp,
        movie_id: MovieId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.insert_watched_movie.execute((id, timestamp, movie_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn move_watched_episode(
        &self,
        id: WatchedId,
        season: api::SeasonNumber,
        episode: u32,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.move_watched_episode.execute((season, episode, id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn orphaned_for_series(
        &self,
        series_id: SeriesId,
    ) -> Result<Vec<api::OrphanedWatched>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_orphaned_for_series.bind((series_id,))?;

            while let Some(r) = stmt.next::<OrphanedWatchedRow>()? {
                out.push(api::OrphanedWatched {
                    id: r.id,
                    timestamp: r.timestamp,
                    series_id: r.series_id,
                    season: r.season,
                    episode: r.episode,
                });
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn remove_watched(&self, id: WatchedId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.delete_watched_episode.execute((id,))?;
            s.delete_watched_movie.execute((id,))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn watched_for_episode(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_watched_by_episode.bind((episode_id,))?;

            while let Some(r) = stmt.next::<WatchedRow>()? {
                out.push(watched_from_row(r)?);
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn watched_for_movie(&self, movie_id: MovieId) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_watched_by_movie.bind((movie_id,))?;

            while let Some(r) = stmt.next::<WatchedRow>()? {
                out.push(watched_from_row(r)?);
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn add_pending_episode(
        &self,
        series_id: api::SeriesId,
        episode_id: api::EpisodeId,
        ts: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.upsert_pending_episode
                .execute((PendingId::random(), ts, series_id, episode_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn add_pending_movie(
        &self,
        movie_id: api::MovieId,
        ts: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.upsert_pending_movie
                .execute((PendingId::random(), ts, movie_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn remove_pending_episode(&self, series_id: api::SeriesId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.delete_pending_episode.execute((series_id,))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn skip_pending_episode(
        &self,
        series_id: api::SeriesId,
        episode_id: api::EpisodeId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            let next_id = s
                .next_episode_after
                .bind((series_id, episode_id))?
                .first::<(api::EpisodeId, Timestamp)>()?
                .map(|r| r.0);

            match next_id {
                Some(next) => {
                    let ts = Timestamp::now();
                    s.upsert_pending_episode
                        .execute((PendingId::random(), ts, series_id, next))?;
                }
                None => {
                    s.delete_pending_episode.execute((series_id,))?;
                }
            }

            Ok(())
        });

        result.await?
    }

    pub(crate) async fn remove_pending_movie(&self, movie_id: api::MovieId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.delete_pending_movie.execute((movie_id,))?;
            Ok(())
        });

        result.await?
    }

    /// Fill the pending slot for a series, but ONLY if it currently has no pending episode.
    /// Called after sync upserts episodes, and after MarkWatched clears the old pending row.
    pub(crate) async fn fill_pending_for_series(
        &self,
        series_id: api::SeriesId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            let stmt = s.has_pending_episode_for_series.bind((series_id,))?;

            let already_has = stmt.first::<(i64,)>()?.is_some();

            if already_has {
                return Ok(());
            }

            let stmt = s.next_pending_episode_for_series.bind((series_id, now))?;

            let Some(row) = stmt.first::<NextEpisodeRow>()? else {
                return Ok(());
            };

            let now = row.aired.unwrap_or(now).max(now);

            s.upsert_pending_episode
                .execute((PendingId::random(), now, series_id, row.id))?;

            Ok(())
        });

        result.await?
    }

    /// Fill the pending slot for a series, but ONLY if it currently has no pending episode.
    /// Called after sync upserts episodes, and after MarkWatched clears the old pending row.
    pub(crate) async fn fill_pending_for_series_from(
        &self,
        series_id: api::SeriesId,
        episode_id: api::EpisodeId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            let already_has = s
                .has_pending_episode_for_series
                .bind((series_id,))?
                .first::<(i64,)>()?
                .is_some();

            if already_has {
                return Ok(());
            }

            let Some((next_id, aired)) = s
                .next_episode_after
                .bind((series_id, episode_id))?
                .first::<(api::EpisodeId, Option<Timestamp>)>()?
            else {
                return Ok(());
            };

            let now = aired.unwrap_or(now).max(now);

            s.upsert_pending_episode
                .execute((PendingId::random(), now, series_id, next_id))?;

            Ok(())
        });

        result.await?
    }

    /// Like `fill_pending_for_series` but for bulk import: finds the first unwatched episode
    /// regardless of whether it has aired, and uses the actual aired timestamp rather than
    /// clamping to `now`. This preserves the episode's original air date as the pending
    /// timestamp so dashboard ordering reflects episode order rather than import time.
    pub(crate) async fn fill_pending_for_series_import(
        &self,
        series_id: api::SeriesId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            let stmt = s.has_pending_episode_for_series.bind((series_id,))?;

            let already_has = stmt.first::<(i64,)>()?.is_some();

            if already_has {
                return Ok(());
            }

            let stmt = s.first_unwatched_episode_for_series.bind((series_id,))?;

            let Some(row) = stmt.first::<NextEpisodeRow>()? else {
                return Ok(());
            };

            let Some(ts) = row.aired else {
                return Ok(());
            };

            s.upsert_pending_episode
                .execute((PendingId::random(), ts, series_id, row.id))?;

            Ok(())
        });

        result.await?
    }

    /// Tracked movies with a passed theatrical release date that are not yet pending or watched.
    pub(crate) async fn theatrical_movie_candidates(
        &self,
        now: Timestamp,
    ) -> Result<Vec<(MovieId, Option<Timestamp>)>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.movies_needing_pending.bind((now,))?;

            while let Some(r) = stmt.next::<PendingMovieCandidateRow>()? {
                out.push((r.id, r.release_date));
            }

            Ok(out)
        });

        result.await?
    }

    /// Tracked movies with a passed digital release date (type 4) that are not yet pending or watched.
    pub(crate) async fn digital_movie_candidates(
        &self,
        now: Timestamp,
    ) -> Result<Vec<(MovieId, Option<Timestamp>)>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.movies_needing_pending_digital.bind((now,))?;

            while let Some(r) = stmt.next::<PendingMovieCandidateRow>()? {
                out.push((r.id, r.release_date));
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn set_series_synced_at(&self, id: SeriesId, at: Timestamp) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;
        let result = spawn_blocking(move || {
            s.set_series_synced_at.execute((at, id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_movie_synced_at(&self, id: MovieId, at: Timestamp) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await;
        let result = spawn_blocking(move || {
            s.set_movie_synced_at.execute((at, id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn upsert_movie_release(
        &self,
        movie_id: MovieId,
        country: &str,
        release_type: ReleaseType,
        timestamp: &Timestamp,
    ) -> Result<()> {
        let country = country.to_owned();
        let timestamp = *timestamp;
        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.upsert_movie_release.execute((
                MovieReleaseId::random(),
                movie_id,
                country.as_str(),
                release_type,
                timestamp,
            ))?;

            Ok(())
        });

        result.await?
    }

    pub(crate) async fn series_needing_sync(
        &self,
        interval_hours: u32,
    ) -> Result<Vec<api::Series>> {
        let cutoff = cutoff_timestamp(interval_hours);
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.series_needing_sync.bind((cutoff,))?;

            while let Some(row) = stmt.next::<SeriesRow>()? {
                out.push(series_from_row(row));
            }

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn movies_needing_sync(&self, interval_hours: u32) -> Result<Vec<api::Movie>> {
        let cutoff = cutoff_timestamp(interval_hours);
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.movies_needing_sync.bind((cutoff,))?;

            while let Some(r) = stmt.next::<MovieRow>()? {
                out.push(movie_from_row(r));
            }

            Ok(out)
        });

        result.await?
    }

    /// Unified pending list replacing pending_episodes + pending_movies.
    pub(crate) async fn pending(&self, now: Timestamp) -> Result<Vec<api::Pending>> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            // Collect the base rows first so the iterating statement is released
            // before the per-row detail lookups below reuse the connection.
            let mut rows = Vec::new();
            let mut stmt = s.list_pending_before.bind((now,))?;

            while let Some(r) = stmt.next::<PendingBaseRow>()? {
                rows.push(r);
            }

            stmt.reset()?;

            'outer: for r in rows {
                let pending = 'pending: {
                    if let Some(episode_id) = r.episode_id {
                        let detail = s
                            .pending_episode_detail
                            .bind((episode_id,))?
                            .first::<PendingEpisodeDetailRow>()?;

                        let Some(d) = detail else {
                            continue 'outer;
                        };

                        let poster = s.image_for_series(d.series_id, ImageKind::Poster)?;
                        let banner = s.image_for_series(d.series_id, ImageKind::Banner)?;

                        break 'pending api::Pending {
                            kind: api::PendingKind::Episode {
                                series: d.series_id,
                                episode: episode_id,
                            },
                            info: api::PendingInfo::Episode {
                                series: d.series_title,
                                episode: d.episode_name,
                                season: d.season,
                                number: d.number,
                            },
                            aired: d.aired,
                            poster,
                            banner,
                        };
                    }

                    if let Some(movie_id) = r.movie_id {
                        let detail = s
                            .pending_movie_detail
                            .bind((movie_id,))?
                            .first::<PendingMovieDetailRow>()?;

                        let Some(d) = detail else {
                            continue 'outer;
                        };

                        let poster = s.image_for_movie(movie_id, ImageKind::Poster)?;
                        let banner = s.image_for_movie(movie_id, ImageKind::Banner)?;

                        break 'pending api::Pending {
                            kind: api::PendingKind::Movie { movie: movie_id },
                            info: api::PendingInfo::Movie { title: d.title },
                            aired: d.release_date,
                            poster,
                            banner,
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

    pub(crate) async fn schedule(
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

        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let mut stmt = s.list_schedule.bind((today, end))?;

            let mut days_map = Vec::<(Date, Vec<(SeriesId, String, Vec<api::Episode>)>)>::new();

            while let Some(r) = stmt.next::<ScheduleRow>()? {
                let Some(day) = r.aired else { continue };

                let ep = api::Episode {
                    id: r.episode_id,
                    series_id: r.series_id,
                    season: r.season,
                    episode: r.number,
                    absolute_number: r.absolute_number,
                    name: r.name,
                    overview: r.overview,
                    aired: r.aired,
                    remote_id: r.remote_id,
                    pending: false,
                    screenshot: None,
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

    pub(crate) async fn load_config(&self) -> Result<Config> {
        let mut s = self.inner.clone().shared().await;

        let result = spawn_blocking(move || {
            let theme = s
                .get_config("theme")?
                .and_then(|v| match v.as_str() {
                    "dark" => Some(ThemeType::Dark),
                    "light" => Some(ThemeType::Light),
                    _ => None,
                })
                .unwrap_or_default();

            let tvdb_api_key = s.get_config("tvdb_api_key")?.unwrap_or_default().to_owned();

            let tvdb_pin = s.get_config("tvdb_pin")?;

            let tmdb_api_key = s.get_config("tmdb_api_key")?.unwrap_or_default().to_owned();

            let dashboard_page = s
                .get_config("dashboard_page")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(5);

            let schedule_duration_days = s
                .get_config("schedule_duration_days")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(7);

            let auto_sync_enabled = s
                .get_config("auto_sync_enabled")?
                .map(|v| v == "true")
                .unwrap_or(false);

            let auto_sync_interval_hours = s
                .get_config("auto_sync_interval_hours")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(24);

            let timezone = s.get_config("timezone")?.unwrap_or_default().to_owned();
            let language = s
                .get_config("language")?
                .filter(|v| !v.is_empty())
                .map(|v| v.to_owned());

            Ok(Config {
                theme,
                tvdb_api_key,
                tvdb_pin,
                tmdb_api_key,
                schedule_duration_days,
                dashboard_page,
                auto_sync_enabled,
                auto_sync_interval_hours,
                timezone,
                language,
            })
        });

        result.await?
    }

    pub(crate) async fn save_config(&self, config: &Config) -> Result<()> {
        let config = config.clone();

        let mut s = self.inner.clone().exclusive().await;

        let result = spawn_blocking(move || {
            s.set_config("theme", config.theme.to_string().as_str())?;

            s.set_config("tvdb_api_key", &config.tvdb_api_key)?;

            if let Some(ref pin) = config.tvdb_pin {
                s.set_config("tvdb_pin", pin)?;
            } else {
                s.delete_config("tvdb_pin")?;
            }

            s.set_config("tmdb_api_key", &config.tmdb_api_key)?;

            s.set_config(
                "schedule_duration_days",
                &config.schedule_duration_days.to_string(),
            )?;

            s.set_config("dashboard_page", &config.dashboard_page.to_string())?;

            s.set_config(
                "auto_sync_enabled",
                if config.auto_sync_enabled {
                    "true"
                } else {
                    "false"
                },
            )?;

            s.set_config(
                "auto_sync_interval_hours",
                &config.auto_sync_interval_hours.to_string(),
            )?;

            s.set_config("timezone", &config.timezone)?;
            s.set_config("language", config.language.as_deref().unwrap_or(""))?;
            Ok(())
        });

        result.await?
    }
}

fn cutoff_timestamp(interval_hours: u32) -> Timestamp {
    let hours = interval_hours.max(1);
    let inner = Timestamp::now().inner();
    let ts = inner
        .checked_sub(jiff::Span::new().hours(hours))
        .unwrap_or(inner);
    Timestamp::from_jiff(ts)
}

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
        poster: None,
        banner: None,
        backdrop: None,
        last_synced_at: r.last_synced_at,
        language: r.language,
    }
}

fn image_from_row(r: ImageRow) -> api::MediaImage {
    api::MediaImage {
        id: r.id,
        kind: r.kind,
        source: r.source,
        image: Image::new(r.source, &r.path),
    }
}

fn series_image_from_row(r: SeriesImageRow) -> api::MediaImage {
    api::MediaImage {
        id: r.id,
        kind: r.kind,
        source: r.source,
        image: Image::new(r.source, &r.path),
    }
}

fn movie_image_from_row(r: MovieImageRow) -> api::MediaImage {
    api::MediaImage {
        id: r.id,
        kind: r.kind,
        source: r.source,
        image: Image::new(r.source, &r.path),
    }
}

fn apply_image_selection(target: &mut api::Series, r: ImageSelectionRow) {
    let image = Image::new_with_dims(r.source, &r.path, r.width, r.height);

    match r.kind {
        api::ImageKind::Poster => target.poster = Some(image),
        api::ImageKind::Banner => target.banner = Some(image),
        api::ImageKind::Backdrop => target.backdrop = Some(image),
        _ => {}
    }
}

fn apply_movie_image_selection(target: &mut api::Movie, sel: ImageSelectionRow) {
    let image = Image::new_with_dims(sel.source, &sel.path, sel.width, sel.height);

    match sel.kind {
        api::ImageKind::Poster => target.poster = Some(image),
        api::ImageKind::Banner => target.banner = Some(image),
        api::ImageKind::Backdrop => target.backdrop = Some(image),
        _ => {}
    }
}

fn season_from_row(r: SeasonRow) -> api::Season {
    api::Season {
        id: r.id,
        series_id: r.series_id,
        season: r.season,
        air_date: r.air_date,
        name: r.name,
        overview: r.overview,
        watched_count: r.watched_count,
        total_count: r.total_count,
    }
}

fn episode_from_row(r: EpisodeRow) -> api::Episode {
    api::Episode {
        id: r.id,
        series_id: r.series_id,
        season: r.season,
        episode: r.number,
        absolute_number: r.absolute_number,
        name: r.name,
        overview: r.overview,
        aired: r.aired,
        remote_id: r.remote_id,
        pending: r.pending,
        screenshot: None,
    }
}

fn watched_episode_from_row(r: WatchedEpisodeRow) -> api::WatchedEpisode {
    api::WatchedEpisode {
        id: r.id,
        timestamp: r.timestamp,
        season: r.season,
        number: r.number,
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
        poster: None,
        banner: None,
        backdrop: None,
        last_synced_at: r.last_synced_at,
        releases: Vec::new(),
        language: r.language,
    }
}

fn watched_from_row(r: WatchedRow) -> Result<api::Watched> {
    let kind = match (r.series_id, r.episode_id, r.movie_id) {
        (Some(series), Some(episode), None) => WatchedKind::Episode { series, episode },
        (None, None, Some(movie)) => WatchedKind::Movie { movie },
        _ => anyhow::bail!("Watched row violates CHECK constraint"),
    };

    Ok(api::Watched {
        id: r.id,
        timestamp: r.timestamp,
        kind,
    })
}

fn do_migrations(c: &sqll::Connection) -> Result<()> {
    c.execute(MIGRATIONS_INIT)?;

    let mut select = c.prepare("SELECT applied_at FROM migrations WHERE id = ?")?;
    let mut insert = c.prepare("INSERT INTO migrations (id, applied_at) VALUES (?, ?)")?;

    for file in Migrations::iter() {
        let id = file.as_ref();

        let result: Result<()> = (|| {
            select.bind(id)?;

            if let Some(applied_at) = select.next::<String>()? {
                tracing::debug!(id, applied_at, "Migration already applied");
                return Ok(());
            }

            let asset =
                Migrations::get(id).with_context(|| anyhow!("Migration file not found: {id}"))?;

            let sql = str::from_utf8(asset.data.as_ref())
                .with_context(|| anyhow!("Migration {id} is not valid UTF-8"))?;

            c.execute(sql)
                .with_context(|| anyhow!("Executing migration {id}"))?;

            let now = Timestamp::now().to_string();
            insert.execute((id, now.as_str()))?;
            tracing::info!(id, "Migration applied");
            Ok(())
        })();

        result.with_context(|| anyhow!("Migration {id}"))?;
    }

    Ok(())
}

fn ensure_mode(c: &sqll::Connection, mode: OpenMode) -> Result<()> {
    // Enforce foreign keys so ON DELETE CASCADE actually fires. Must run
    // outside any transaction.
    c.execute("PRAGMA foreign_keys = ON;")?;

    match mode {
        OpenMode::Normal => {
            // NORMAL is the recommended companion to WAL: still crash-safe
            // against corruption and application crashes, only fsyncing at
            // checkpoints rather than on every commit. A committed transaction
            // can be lost only on OS crash / power loss, never corrupting the
            // db.
            //
            // Unlike journal_mode, synchronous is per-connection and not
            // persisted in the database header, so it resets to the default
            // (FULL) on every open and must simply be set unconditionally.
            c.execute("PRAGMA journal_mode = wal;")?;
            c.execute("PRAGMA synchronous = normal;")?;
            c.execute("PRAGMA busy_timeout = 5000;")?;
        }
        OpenMode::Bulk => {
            c.execute("PRAGMA journal_mode = off;")?;
            c.execute("PRAGMA synchronous = off;")?;
        }
    }

    Ok(())
}
