#![allow(clippy::too_many_arguments)]

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use std::collections::HashMap;

use api::{
    Config, Date, EpisodeId, Image, ImageId, ImageKind, ImageSource, MovieId, RemoteId, SeasonId,
    SeasonNumber, SeriesId, ThemeType, Timestamp, WatchedId, WatchedKind,
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
    title: String,
    first_air: Option<Date>,
    overview: String,
    tracked: bool,
    pending_episode_id: Option<EpisodeId>,
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
    number: i64,
    air_date: Option<Date>,
    name: Option<String>,
    overview: String,
    poster: Option<Image>,
}

#[derive(Row)]
struct EpisodeRow {
    id: EpisodeId,
    series_id: SeriesId,
    season: i64,
    number: i64,
    absolute_number: Option<i64>,
    name: Option<String>,
    overview: String,
    aired: Option<Date>,
    filename: Option<Image>,
    remote_id: Option<RemoteId>,
    watched: bool,
    watched_count: i64,
    last_watched_id: Option<WatchedId>,
}

#[derive(Row)]
struct MovieRow {
    id: MovieId,
    title: String,
    release_date: Option<Date>,
    overview: String,
    watched: bool,
    watched_count: i64,
    pending: bool,
    tracked: bool,
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
struct InsertWatchedRow {
    id: WatchedId,
    timestamp: Timestamp,
}

#[derive(Row)]
struct PendingEpisodeRow {
    series_id: SeriesId,
    episode_id: EpisodeId,
    aired: Option<Date>,
    series_title: String,
    episode_name: Option<String>,
    season: i64,
    number: i64,
    poster: Option<Image>,
}

#[derive(Row)]
struct PendingMovieRow {
    movie_id: MovieId,
    title: String,
    release_date: Option<Date>,
    poster: Option<Image>,
}

#[derive(Row)]
struct ScheduleRow {
    series_id: SeriesId,
    series_title: String,
    episode_id: EpisodeId,
    season: i64,
    number: i64,
    absolute_number: Option<i64>,
    name: Option<String>,
    overview: String,
    aired: Option<Date>,
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
            RETURNING id, title, first_air, overview, tracked, NULL AS pending_episode_id
        "#,
        list_series: r#"
            SELECT id, title, first_air, overview, tracked, pending_episode_id
            FROM series ORDER BY title
        "#,
        series_by_id: r#"
            SELECT id, title, first_air, overview, tracked, pending_episode_id
            FROM series WHERE id = ?
        "#,
        series_by_remote: r#"
            SELECT s.id, s.title, s.first_air, s.overview, s.tracked, s.pending_episode_id
            FROM series s
            JOIN remotes r ON r.series_id = s.id
            WHERE r.remote_id = ?
        "#,
        set_series_next_episode: r#"
            UPDATE series SET pending_episode_id = ? WHERE id = ?
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
            RETURNING id, series_id, season, number, absolute_number, name, overview, aired, filename, remote_id,
                      0 AS watched, 0 AS watched_count, NULL AS last_watched_id
        "#,
        list_episodes: r#"
            SELECT e.id, e.series_id, e.season, e.number, e.absolute_number, e.name, e.overview,
                   e.aired, e.filename, e.remote_id,
                   (SELECT COUNT(*) FROM watched w WHERE w.episode_id = e.id) > 0 AS watched,
                   (SELECT COUNT(*) FROM watched w WHERE w.episode_id = e.id) AS watched_count,
                   (SELECT w.id FROM watched w WHERE w.episode_id = e.id ORDER BY w.id DESC LIMIT 1) AS last_watched_id
            FROM episodes e
            WHERE e.series_id = ? AND e.season = ?
            ORDER BY e.number
        "#,
        episode_by_id: r#"
            SELECT e.id, e.series_id, e.season, e.number, e.absolute_number, e.name, e.overview,
                   e.aired, e.filename, e.remote_id,
                   (SELECT COUNT(*) FROM watched w WHERE w.episode_id = e.id) > 0 AS watched,
                   (SELECT COUNT(*) FROM watched w WHERE w.episode_id = e.id) AS watched_count,
                   (SELECT w.id FROM watched w WHERE w.episode_id = e.id ORDER BY w.id DESC LIMIT 1) AS last_watched_id
            FROM episodes e WHERE e.id = ?
        "#,

        // movies
        insert_movie: r#"
            INSERT INTO movies (title, release_date, overview, tracked)
            VALUES (?, ?, ?, ?)
            RETURNING id, title, release_date, overview, 0 AS watched, 0 AS watched_count, 0 AS pending, tracked
        "#,
        list_movies: r#"
            SELECT m.id, m.title, m.release_date, m.overview,
                   (SELECT COUNT(*) FROM watched w WHERE w.movie_id = m.id) > 0 AS watched,
                   (SELECT COUNT(*) FROM watched w WHERE w.movie_id = m.id) AS watched_count,
                   m.pending, m.tracked
            FROM movies m ORDER BY m.title
        "#,
        movie_by_id: r#"
            SELECT m.id, m.title, m.release_date, m.overview,
                   (SELECT COUNT(*) FROM watched w WHERE w.movie_id = m.id) > 0 AS watched,
                   (SELECT COUNT(*) FROM watched w WHERE w.movie_id = m.id) AS watched_count,
                   m.pending, m.tracked
            FROM movies m WHERE m.id = ?
        "#,
        movie_by_remote: r#"
            SELECT m.id, m.title, m.release_date, m.overview,
                   (SELECT COUNT(*) FROM watched w WHERE w.movie_id = m.id) > 0 AS watched,
                   (SELECT COUNT(*) FROM watched w WHERE w.movie_id = m.id) AS watched_count,
                   m.pending, m.tracked
            FROM movies m
            JOIN remotes r ON r.movie_id = m.id
            WHERE r.remote_id = ?
        "#,
        set_movie_pending: r#"
            UPDATE movies SET pending = ? WHERE id = ?
        "#,
        set_movie_tracked: r#"
            UPDATE movies SET tracked = ? WHERE id = ?
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

        // pending (unwatched aired episodes + unwatched movies)
        list_pending_episodes: r#"
            SELECT e.series_id, e.id AS episode_id, e.aired,
                   s.title AS series_title,
                   e.name AS episode_name,
                   e.season, e.number,
                   (SELECT source || ':' || path FROM images
                    WHERE series_id = s.id AND kind = 'poster' AND selected = 1 LIMIT 1) AS poster
            FROM series s
            JOIN episodes e ON e.id = s.pending_episode_id
            WHERE s.tracked = 1
              AND s.pending_episode_id IS NOT NULL

            UNION ALL

            SELECT e.series_id, e.id AS episode_id, e.aired,
                   s.title AS series_title,
                   e.name AS episode_name,
                   e.season, e.number,
                   (SELECT source || ':' || path FROM images
                    WHERE series_id = s.id AND kind = 'poster' AND selected = 1 LIMIT 1) AS poster
            FROM episodes e
            JOIN series s ON s.id = e.series_id
            WHERE s.tracked = 1
              AND s.pending_episode_id IS NULL
              AND e.aired IS NOT NULL
              AND e.aired <= ?
              AND NOT EXISTS (SELECT 1 FROM watched w WHERE w.episode_id = e.id)
              AND NOT EXISTS (
                SELECT 1 FROM episodes e2
                WHERE e2.series_id = e.series_id
                  AND e2.aired IS NOT NULL
                  AND e2.aired <= ?
                  AND NOT EXISTS (SELECT 1 FROM watched w2 WHERE w2.episode_id = e2.id)
                  AND (e2.season < e.season OR (e2.season = e.season AND e2.number < e.number))
              )

            ORDER BY aired DESC, series_title, season, number
        "#,
        list_pending_movies: r#"
            SELECT m.id AS movie_id, m.title, m.release_date,
                   (SELECT source || ':' || path FROM images
                    WHERE movie_id = m.id AND kind = 'poster' AND selected = 1 LIMIT 1) AS poster
            FROM movies m
            WHERE (
                m.release_date IS NOT NULL
                AND m.release_date <= ?
                AND NOT EXISTS (SELECT 1 FROM watched w WHERE w.movie_id = m.id)
            ) OR m.pending = 1
            ORDER BY m.release_date DESC, m.title
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
        first_air: Option<&Date>,
        overview: &str,
    ) -> Result<api::Series> {
        let title = title.to_owned();
        let first_air = first_air.cloned();
        let overview = overview.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.insert_series
                .bind((&title[..], first_air.as_ref(), &overview[..], true))?;

            let r = s
                .insert_series
                .next::<SeriesRow>()?
                .context("insert_series returned no row")?;

            ensure!(s.insert_series.step()?.is_done(), "insert_series");
            Ok(series_from_row(r))
        })
        .await?
    }

    pub async fn add_series_remote(&self, series_id: SeriesId, remote_id: &RemoteId) -> Result<()> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.insert_series_remote.bind((series_id, &remote_id))?;
            ensure!(
                s.insert_series_remote.step()?.is_done(),
                "insert_series_remote"
            );
            Ok(())
        })
        .await?
    }

    pub async fn series(&self) -> Result<Vec<api::Series>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
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
        })
        .await?
    }

    pub async fn series_by_id(&self, id: SeriesId) -> Result<Option<api::Series>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
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
        })
        .await?
    }

    pub async fn update_series(
        &self,
        id: SeriesId,
        title: &str,
        first_air: Option<&Date>,
        overview: &str,
        tracked: bool,
    ) -> Result<()> {
        let title = title.to_owned();
        let first_air = first_air.cloned();
        let overview = overview.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.update_series
                .bind((&title[..], first_air.as_ref(), &overview[..], tracked, id))?;
            ensure!(s.update_series.step()?.is_done(), "update_series");
            Ok(())
        })
        .await?
    }

    pub async fn delete_series(&self, id: SeriesId) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.delete_series.bind((id,))?;
            ensure!(s.delete_series.step()?.is_done(), "delete_series");
            Ok(())
        })
        .await?
    }

    pub async fn set_series_tracked(&self, id: SeriesId, tracked: bool) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.set_series_tracked.bind((tracked, id))?;
            ensure!(s.set_series_tracked.step()?.is_done(), "set_series_tracked");
            Ok(())
        })
        .await?
    }

    pub async fn set_series_next_episode(
        &self,
        series_id: SeriesId,
        episode_id: Option<EpisodeId>,
    ) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.set_series_next_episode.bind((episode_id, series_id))?;
            ensure!(
                s.set_series_next_episode.step()?.is_done(),
                "set_series_next_episode"
            );
            Ok(())
        })
        .await?
    }

    // ── Seasons ──

    pub async fn upsert_season(
        &self,
        series_id: SeriesId,
        number: SeasonNumber,
        air_date: Option<&Date>,
        name: Option<&str>,
        overview: &str,
        poster: Option<&Image>,
    ) -> Result<api::Season> {
        let air_date = air_date.cloned();
        let name = name.map(str::to_owned);
        let overview = overview.to_owned();
        let poster = poster.cloned();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.upsert_season.bind((
                series_id,
                number.to_i64(),
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
        })
        .await?
    }

    pub async fn seasons(&self, series_id: SeriesId) -> Result<Vec<api::Season>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.list_seasons.bind((series_id,))?;
            let mut out = Vec::new();
            while let Some(r) = s.list_seasons.next::<SeasonRow>()? {
                out.push(season_from_row(r));
            }
            Ok(out)
        })
        .await?
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
        aired: Option<&Date>,
        filename: Option<&Image>,
        remote_id: Option<&RemoteId>,
    ) -> Result<api::Episode> {
        let name = name.map(str::to_owned);
        let overview = overview.to_owned();
        let aired = aired.cloned();
        let filename = filename.cloned();
        let remote_id = remote_id.cloned();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.upsert_episode.bind((
                series_id,
                season.to_i64(),
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
        })
        .await?
    }

    pub async fn episodes(
        &self,
        series_id: SeriesId,
        season: SeasonNumber,
    ) -> Result<Vec<api::Episode>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.list_episodes.bind((series_id, season.to_i64()))?;
            let mut out = Vec::new();
            while let Some(r) = s.list_episodes.next::<EpisodeRow>()? {
                out.push(episode_from_row(r));
            }
            Ok(out)
        })
        .await?
    }

    pub async fn episode_by_id(&self, id: EpisodeId) -> Result<Option<api::Episode>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.episode_by_id.bind((id,))?;
            Ok(s.episode_by_id.next::<EpisodeRow>()?.map(episode_from_row))
        })
        .await?
    }

    // ── Movies ──

    pub async fn create_movie(
        &self,
        title: &str,
        release_date: Option<Date>,
        overview: &str,
        tracked: bool,
    ) -> Result<api::Movie> {
        let title = title.to_owned();
        let overview = overview.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.insert_movie
                .bind((&title[..], release_date, &overview[..], tracked))?;

            let r = s
                .insert_movie
                .next::<MovieRow>()?
                .context("insert_movie returned no row")?;

            ensure!(s.insert_movie.step()?.is_done(), "insert_movie");
            Ok(movie_from_row(r))
        })
        .await?
    }

    pub async fn add_movie_remote(&self, movie_id: MovieId, remote_id: &RemoteId) -> Result<()> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.insert_movie_remote.bind((movie_id, &remote_id))?;
            ensure!(
                s.insert_movie_remote.step()?.is_done(),
                "insert_movie_remote"
            );
            Ok(())
        })
        .await?
    }

    pub async fn movies(&self) -> Result<Vec<api::Movie>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
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
        })
        .await?
    }

    pub async fn movie_by_id(&self, id: MovieId) -> Result<Option<api::Movie>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
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
            Ok(Some(movie))
        })
        .await?
    }

    pub async fn series_by_remote_id(&self, remote_id: &RemoteId) -> Result<Option<api::Series>> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
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
        })
        .await?
    }

    pub async fn movie_by_remote_id(&self, remote_id: &RemoteId) -> Result<Option<api::Movie>> {
        let remote_id = remote_id.clone();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
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
        })
        .await?
    }

    pub async fn update_movie(
        &self,
        id: MovieId,
        title: &str,
        release_date: Option<&Date>,
        overview: &str,
    ) -> Result<()> {
        let title = title.to_owned();
        let release_date = release_date.cloned();
        let overview = overview.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.update_movie
                .bind((&title[..], release_date.as_ref(), &overview[..], id))?;
            ensure!(s.update_movie.step()?.is_done(), "update_movie");
            Ok(())
        })
        .await?
    }

    pub async fn delete_movie(&self, id: MovieId) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.delete_movie.bind((id,))?;
            ensure!(s.delete_movie.step()?.is_done(), "delete_movie");
            Ok(())
        })
        .await?
    }

    pub async fn set_movie_pending(&self, id: MovieId, pending: bool) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.set_movie_pending.bind((pending, id))?;
            ensure!(s.set_movie_pending.step()?.is_done(), "set_movie_pending");
            Ok(())
        })
        .await?
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

        spawn_blocking(move || {
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
        })
        .await?
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

        spawn_blocking(move || {
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
        })
        .await?
    }

    /// Deselects all images of the same kind for the owning entity, then
    /// selects the given image. Returns which entity owns the image.
    pub async fn select_image(&self, id: ImageId) -> Result<api::ImageOwner> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
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
        })
        .await?
    }

    // ── Watched ──

    pub async fn mark_watched(
        &self,
        kind: WatchedKind,
        timestamp: Timestamp,
    ) -> Result<api::Watched> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
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
        })
        .await?
    }

    pub async fn remove_watched(&self, id: WatchedId) -> Result<()> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.delete_watched.bind((id,))?;
            ensure!(s.delete_watched.step()?.is_done(), "delete_watched");
            Ok(())
        })
        .await?
    }

    pub async fn all_watched(&self) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.list_all_watched.reset()?;
            let mut out = Vec::new();
            while let Some(r) = s.list_all_watched.next::<WatchedRow>()? {
                out.push(watched_from_row(r)?);
            }
            Ok(out)
        })
        .await?
    }

    pub async fn watched_for_episode(&self, episode_id: EpisodeId) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.list_watched_episode.bind((episode_id,))?;
            let mut out = Vec::new();
            while let Some(r) = s.list_watched_episode.next::<WatchedRow>()? {
                out.push(watched_from_row(r)?);
            }
            Ok(out)
        })
        .await?
    }

    pub async fn watched_for_movie(&self, movie_id: MovieId) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.list_watched_movie.bind((movie_id,))?;
            let mut out = Vec::new();
            while let Some(r) = s.list_watched_movie.next::<WatchedRow>()? {
                out.push(watched_from_row(r)?);
            }
            Ok(out)
        })
        .await?
    }

    // ── Dashboard queries ──

    pub async fn pending_episodes(&self, limit: u32) -> Result<Vec<api::Pending>> {
        let today = api::Date::today();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.list_pending_episodes.bind((today, today))?;
            let mut out = Vec::new();

            while let Some(r) = s.list_pending_episodes.next::<PendingEpisodeRow>()? {
                if out.len() >= limit as usize {
                    break;
                }
                let label = match r.episode_name {
                    Some(ref name) => format!("S{:02}E{:02} – {}", r.season, r.number, name),
                    None => format!("S{:02}E{:02}", r.season, r.number),
                };
                out.push(api::Pending {
                    kind: api::PendingKind::Episode {
                        series: r.series_id,
                        episode: r.episode_id,
                    },
                    aired: r.aired,
                    series_title: Some(r.series_title),
                    label,
                    poster: r.poster,
                });
            }

            s.list_pending_episodes.reset()?;
            Ok(out)
        })
        .await?
    }

    pub async fn pending_movies(&self) -> Result<Vec<api::Pending>> {
        let today = api::Date::today();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.list_pending_movies.bind((today,))?;
            let mut out = Vec::new();
            while let Some(r) = s.list_pending_movies.next::<PendingMovieRow>()? {
                out.push(api::Pending {
                    kind: api::PendingKind::Movie { movie: r.movie_id },
                    aired: r.release_date,
                    series_title: None,
                    label: r.title,
                    poster: r.poster,
                });
            }
            Ok(out)
        })
        .await?
    }

    pub async fn schedule(&self, days: u32) -> Result<Vec<api::ScheduledDay>> {
        let today = api::Date::today();
        let end = today.checked_add_days(days as i32);
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.list_schedule.bind((today, end))?;

            let mut days_map = Vec::<(Date, Vec<(SeriesId, String, Vec<api::Episode>)>)>::new();

            while let Some(r) = s.list_schedule.next::<ScheduleRow>()? {
                let Some(day) = r.aired else { continue };

                let ep = api::Episode {
                    id: r.episode_id,
                    series_id: r.series_id,
                    season: SeasonNumber::from_i64(r.season),
                    number: r.number as u32,
                    absolute_number: r.absolute_number.map(|n| n as u32),
                    name: r.name,
                    overview: r.overview,
                    aired: r.aired,
                    filename: r.filename,
                    remote_id: r.remote_id,
                    watched: false,
                    watched_count: 0,
                    last_watched_id: None,
                };

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
        })
        .await?
    }

    // ── Config ──

    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.get_config.bind((key.as_str(),))?;
            Ok(s.get_config.next::<ConfigRow>()?.map(|r| r.value))
        })
        .await?
    }

    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let key = key.to_owned();
        let value = value.to_owned();
        let mut s = self.inner.clone().lock_owned().await;

        spawn_blocking(move || {
            s.set_config.bind((key.as_str(), value.as_str()))?;
            ensure!(s.set_config.step()?.is_done(), "set_config");
            Ok(())
        })
        .await?
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

        Ok(Config {
            theme,
            tvdb_legacy_apikey,
            tmdb_api_key,
            schedule_duration_days,
            dashboard_limit,
            dashboard_page,
            auto_sync_enabled,
            auto_sync_interval_hours,
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
        Ok(())
    }
}

// ── Row converters ───────────────────────────────────────────────────────────

fn series_from_row(r: SeriesRow) -> api::Series {
    api::Series {
        id: r.id,
        title: r.title,
        first_air_date: r.first_air,
        overview: r.overview,
        tracked: r.tracked,
        remotes: Vec::new(),
        pending_episode_id: r.pending_episode_id,
        images: Vec::new(),
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
        number: SeasonNumber::from_i64(r.number),
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
        season: SeasonNumber::from_i64(r.season),
        number: r.number as u32,
        absolute_number: r.absolute_number.map(|n| n as u32),
        name: r.name,
        overview: r.overview,
        aired: r.aired,
        filename: r.filename,
        remote_id: r.remote_id,
        watched: r.watched,
        watched_count: r.watched_count as u32,
        last_watched_id: r.last_watched_id,
    }
}

fn movie_from_row(r: MovieRow) -> api::Movie {
    api::Movie {
        id: r.id,
        title: r.title,
        release_date: r.release_date,
        overview: r.overview,
        remotes: Vec::new(),
        watched: r.watched,
        tracked: r.tracked,
        watched_count: r.watched_count as u32,
        pending: r.pending,
        images: Vec::new(),
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
