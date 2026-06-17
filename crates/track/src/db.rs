#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

use core::ops::{Deref, DerefMut};
use core::str;

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use std::collections::{HashMap, HashSet};

use api::{
    Config, Date, EpisodeId, Image, ImageId, ImageKind, ImageSource, MarkTime, MovieId,
    MovieReleaseId, PendingId, ReleaseType, Remote, RemoteId, RemoteSource, RemoteValue, SeasonId,
    SeasonNumber, ShowId, ThemeType, Timestamp, WatchedId, WatchedKind,
};
use rust_embed::RustEmbed;
use sqll::{OpenOptions, Pool, PoolBuilder, Row, Statements, TypedStatement};
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

#[derive(Row)]
struct ShowRow {
    id: ShowId,
    title: Option<String>,
    first_air: Option<Timestamp>,
    overview: Option<String>,
    tracked: bool,
    auto_sync: bool,
    last_synced_at: Option<Timestamp>,
    // Legacy per-show settings columns (dormant once a `show_settings` row exists).
    language: Option<String>,
    include_specials: Option<bool>,
    air_date_filters: Option<String>,
    // The `show_settings.data` JSON blob, when present, supersedes the columns above.
    settings_data: Option<String>,
}

/// A media row reduced to just its custom-language inputs: the legacy `language`
/// column and the settings JSON blob (which, when present, supersedes it). Used to
/// tally the most-used per-show/per-movie language overrides.
#[derive(Row)]
struct LanguageRow {
    language: Option<String>,
    settings_data: Option<String>,
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
    show_id: Option<ShowId>,
    movie_id: Option<MovieId>,
    season_id: Option<SeasonId>,
}

#[derive(Row)]
struct ShowImageRow {
    id: ImageId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    show_id: ShowId,
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
struct AllShowImageSelectionRow {
    show_id: ShowId,
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
    show_id: ShowId,
    season: SeasonNumber,
    air_date: Option<Timestamp>,
    name: Option<String>,
    overview: Option<String>,
    poster_source: Option<ImageSource>,
    poster_path: Option<String>,
    watched_count: u32,
    total_count: u32,
}

#[derive(Row)]
struct EpisodeRow {
    id: EpisodeId,
    show_id: ShowId,
    season: SeasonNumber,
    number: u32,
    absolute_number: Option<u32>,
    name: Option<String>,
    overview: Option<String>,
    aired: Option<Timestamp>,
    pending: bool,
    watched_count: u32,
}

/// A remote (source + value) attached to an episode, fetched separately and
/// joined onto episodes in Rust by `episode_id`.
#[derive(Row)]
struct EpisodeRemoteRow {
    episode_id: EpisodeId,
    source: RemoteSource,
    value: RemoteValue,
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
    auto_sync: bool,
    last_synced_at: Option<Timestamp>,
    // Legacy per-movie settings columns (dormant once a `movie_settings` row exists).
    language: Option<String>,
    release_filters: Option<String>,
    // The `movie_settings.data` JSON blob, when present, supersedes the columns above.
    settings_data: Option<String>,
}

#[derive(Row)]
struct MovieReleaseRow {
    country: String,
    release_type: ReleaseType,
    timestamp: Timestamp,
}

/// One stored air date for an episode (joined back onto its episode in Rust).
#[derive(Row)]
struct EpisodeReleaseRow {
    episode_id: EpisodeId,
    source: RemoteSource,
    country: String,
    network: String,
    timestamp: Timestamp,
}

#[derive(Row)]
struct MediaItemRow {
    // SQLite stores ids as signed integers; reinterpret to u64 (matches `define_id`).
    id: i64,
    title: Option<String>,
    date: Option<Timestamp>,
    overview: Option<String>,
    tracked: bool,
}

#[derive(Row)]
struct LastWatchedMovieRow {
    movie_id: MovieId,
    last_watched: Timestamp,
}

#[derive(Row)]
struct LastWatchedShowRow {
    show_id: ShowId,
    last_watched: Timestamp,
}

#[derive(Row)]
struct WatchedRow {
    id: WatchedId,
    timestamp: Timestamp,
    episode_id: Option<EpisodeId>,
    movie_id: Option<MovieId>,
    show_id: Option<ShowId>,
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
    show_id: ShowId,
    season: SeasonNumber,
    episode: u32,
}

#[derive(Row)]
struct EpisodeNaturalKeyRow {
    show_id: ShowId,
    season: SeasonNumber,
    number: u32,
}

#[derive(Row)]
struct UnwatchedEpisodeRow {
    id: EpisodeId,
    show_id: ShowId,
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
    show_id: api::ShowId,
    show_title: Option<String>,
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
struct PendingEpisodeAiredRow {
    aired: Option<Timestamp>,
    timestamp: Timestamp,
}

#[derive(Row)]
struct MoviePendingCandidateRow {
    id: api::MovieId,
    release_filters: Option<String>,
}

#[derive(Row)]
struct ScheduleRow {
    show_id: ShowId,
    show_title: String,
    season: SeasonNumber,
    number: u32,
    aired: Option<Timestamp>,
}

#[derive(Row)]
struct ScheduleMovieRow {
    movie_id: MovieId,
    title: String,
    released: Option<Timestamp>,
}

/// A single stored remote (`id`, `source`, `value`) for one show/movie.
#[derive(Row)]
struct RemoteRow {
    id: RemoteId,
    slug: Option<String>,
    source: RemoteSource,
    value: RemoteValue,
    enabled: bool,
    priority: i32,
    sync_kinds: Option<api::SyncKindSet>,
}

/// A remote owned by a show (`list_all_show_remotes`) or movie
/// (`list_all_movie_remotes`), grouped onto its owner in Rust.
#[derive(Row)]
struct AllShowRemoteRow {
    show_id: ShowId,
    id: RemoteId,
    slug: Option<String>,
    source: RemoteSource,
    value: RemoteValue,
    enabled: bool,
    priority: i32,
    sync_kinds: Option<api::SyncKindSet>,
}

#[derive(Row)]
struct AllMovieRemoteRow {
    movie_id: MovieId,
    id: RemoteId,
    slug: Option<String>,
    source: RemoteSource,
    value: RemoteValue,
    enabled: bool,
    priority: i32,
    sync_kinds: Option<api::SyncKindSet>,
}

#[derive(Statements)]
#[sql(read_only)]
struct InnerRead {
    // shows
    #[sql = "SELECT shows.id, title, first_air, overview, tracked, auto_sync, last_synced_at, language, include_specials, air_date_filters, show_settings.data AS settings_data"]
    #[sql = "FROM shows LEFT JOIN show_settings ON show_settings.show_id = shows.id ORDER BY title"]
    list_shows: TypedStatement<(), ShowRow>,
    #[sql = "SELECT shows.id, title, first_air, overview, tracked, auto_sync, last_synced_at, language, include_specials, air_date_filters, show_settings.data AS settings_data"]
    #[sql = "FROM shows LEFT JOIN show_settings ON show_settings.show_id = shows.id WHERE shows.id = ?"]
    show_by_id: TypedStatement<(ShowId,), ShowRow>,
    #[sql = "SELECT s.id, s.title, s.first_air, s.overview, s.tracked, s.auto_sync, s.last_synced_at, s.language, s.include_specials, s.air_date_filters, ss.data AS settings_data"]
    #[sql = "FROM shows s"]
    #[sql = "JOIN show_remotes r ON r.show_id = s.id"]
    #[sql = "LEFT JOIN show_settings ss ON ss.show_id = s.id"]
    #[sql = "WHERE r.source = ? AND r.value = ?"]
    shows_by_remote: TypedStatement<(RemoteSource, RemoteValue), ShowRow>,

    // remotes (one table per owner; source is a numeric enum, value is dynamic)
    #[sql = "SELECT id, slug, source, value, enabled, priority, sync_kinds FROM show_remotes WHERE show_id = ? ORDER BY priority, id"]
    list_show_remotes: TypedStatement<(ShowId,), RemoteRow>,
    #[sql = "SELECT show_id, id, slug, source, value, enabled, priority, sync_kinds FROM show_remotes ORDER BY show_id, priority, id"]
    list_all_show_remotes: TypedStatement<(), AllShowRemoteRow>,
    #[sql = "SELECT show_id FROM show_remotes WHERE source = ? AND value = ? LIMIT 1"]
    show_id_by_remote: TypedStatement<(RemoteSource, RemoteValue), ShowId>,

    // images (shows and movies share one table)
    #[sql = "SELECT id, kind, source, path FROM images"]
    #[sql = "WHERE show_id = ? ORDER BY kind, rank, id"]
    list_show_images: TypedStatement<(ShowId,), ImageRow>,
    #[sql = "SELECT id, kind, source, path, show_id FROM images"]
    #[sql = "WHERE show_id IS NOT NULL ORDER BY show_id, kind, rank, id"]
    list_all_show_images: TypedStatement<(), ShowImageRow>,
    #[sql = "SELECT ei.episode_id, i.source, i.path, i.width, i.height"]
    #[sql = "FROM episode_images ei JOIN images i ON i.id = ei.image_id"]
    #[sql = "WHERE ei.kind = ? AND ei.episode_id IN (SELECT id FROM episodes WHERE show_id = ? AND season = ?)"]
    list_season_episode_screenshots:
        TypedStatement<(ImageKind, ShowId, SeasonNumber), EpisodeScreenshotRow>,
    #[sql = "SELECT id, kind, source, path FROM images"]
    #[sql = "WHERE movie_id = ? ORDER BY kind, rank, id"]
    list_movie_images: TypedStatement<(MovieId,), ImageRow>,
    #[sql = "SELECT id, kind, source, path, movie_id FROM images"]
    #[sql = "WHERE movie_id IS NOT NULL ORDER BY movie_id, kind, rank, id"]
    list_all_movie_images: TypedStatement<(), MovieImageRow>,
    #[sql = "SELECT kind, show_id, movie_id, season_id FROM images WHERE id = ?"]
    image_by_id: TypedStatement<(ImageId,), ImageMetaRow>,
    #[sql = "SELECT id, kind, source, path FROM images"]
    #[sql = "WHERE season_id = ? ORDER BY kind, rank, id"]
    list_season_images: TypedStatement<(SeasonId,), ImageRow>,
    #[sql = "SELECT show_id FROM seasons WHERE id = ?"]
    show_id_for_season: TypedStatement<(SeasonId,), ShowId>,

    // selection tables
    #[sql = "SELECT si.kind, i.source, i.path, i.width, i.height"]
    #[sql = "FROM show_images si JOIN images i ON i.id = si.image_id"]
    #[sql = "WHERE si.show_id = ?"]
    list_show_image_selections: TypedStatement<(ShowId,), ImageSelectionRow>,
    #[sql = "SELECT si.show_id, si.kind, i.source, i.path, i.width, i.height"]
    #[sql = "FROM show_images si JOIN images i ON i.id = si.image_id"]
    list_all_show_image_selections: TypedStatement<(), AllShowImageSelectionRow>,
    #[sql = "SELECT mi.kind, i.source, i.path, i.width, i.height"]
    #[sql = "FROM movie_images mi JOIN images i ON i.id = mi.image_id"]
    #[sql = "WHERE mi.movie_id = ?"]
    list_movie_image_selections: TypedStatement<(MovieId,), ImageSelectionRow>,
    #[sql = "SELECT mi.movie_id, mi.kind, i.source, i.path, i.width, i.height"]
    #[sql = "FROM movie_images mi JOIN images i ON i.id = mi.image_id"]
    list_all_movie_image_selections: TypedStatement<(), AllMovieImageSelectionRow>,

    // seasons
    #[sql = "SELECT s.id, s.show_id, s.season, s.air_date, s.name, s.overview,"]
    #[sql = "    i.source AS poster_source, i.path AS poster_path,"]
    #[sql = "    (SELECT COUNT(DISTINCT we.episode) FROM watched_episodes we WHERE we.show_id = s.show_id AND we.season = s.season) AS watched_count,"]
    #[sql = "    (SELECT COUNT(*) FROM episodes e WHERE e.show_id = s.show_id AND e.season = s.season) AS total_count"]
    #[sql = "FROM seasons s"]
    #[sql = "LEFT JOIN season_images si ON si.season_id = s.id AND si.kind = 1"]
    #[sql = "LEFT JOIN images i ON i.id = si.image_id"]
    #[sql = "WHERE s.show_id = ? ORDER BY s.season"]
    list_seasons: TypedStatement<(ShowId,), SeasonRow>,
    #[sql = "SELECT episode FROM episodes WHERE show_id = ? AND season = ?"]
    episode_numbers_for_season: TypedStatement<(ShowId, SeasonNumber), u32>,

    // episodes
    #[sql = "SELECT show_id, season, episode FROM episodes WHERE id = ?"]
    episode_natural_key: TypedStatement<(EpisodeId,), EpisodeNaturalKeyRow>,
    #[sql = "SELECT id, season, episode FROM episodes WHERE show_id = ?"]
    list_episode_ids_for_show: TypedStatement<(ShowId,), EpisodeIdRow>,
    #[sql = "SELECT e.id, e.show_id, e.season, e.episode, e.absolute_number, e.name, e.overview, e.aired,"]
    #[sql = "    EXISTS(SELECT 1 FROM pending p WHERE p.episode_id = e.id) AS pending,"]
    #[sql = "    (SELECT COUNT(*) FROM watched_episodes we WHERE we.show_id = e.show_id AND we.season = e.season AND we.episode = e.episode) AS watched_count"]
    #[sql = "FROM episodes e"]
    #[sql = "WHERE e.show_id = ? AND e.season = ?"]
    #[sql = "ORDER BY e.episode"]
    list_episodes: TypedStatement<(ShowId, SeasonNumber), EpisodeRow>,
    #[sql = "SELECT er.episode_id, er.source, er.value"]
    #[sql = "FROM episode_remotes er"]
    #[sql = "JOIN episodes e ON e.id = er.episode_id"]
    #[sql = "WHERE e.show_id = ? AND e.season = ?"]
    list_season_episode_remotes: TypedStatement<(ShowId, SeasonNumber), EpisodeRemoteRow>,
    #[sql = "SELECT we.id, we.timestamp, we.season, we.episode, e.id AS episode_id"]
    #[sql = "FROM watched_episodes we"]
    #[sql = "JOIN episodes e ON e.show_id = we.show_id AND e.season = we.season AND e.episode = we.episode"]
    #[sql = "WHERE we.show_id = ?"]
    #[sql = "ORDER BY we.timestamp DESC"]
    list_episodes_watched: TypedStatement<(ShowId,), WatchedEpisodeRow>,
    #[sql = "SELECT aired FROM episodes WHERE id = ?"]
    episode_aired_by_id: TypedStatement<(EpisodeId,), Option<Timestamp>>,

    // slim list views
    #[sql = "SELECT id, title, release_date AS date, overview, tracked FROM movies ORDER BY title"]
    list_movie_items: TypedStatement<(), MediaItemRow>,
    #[sql = "SELECT id, title, first_air AS date, overview, tracked FROM shows ORDER BY title"]
    list_show_items: TypedStatement<(), MediaItemRow>,
    #[sql = "SELECT movie_id, MAX(timestamp) AS last_watched FROM watched_movies"]
    #[sql = "WHERE movie_id IS NOT NULL GROUP BY movie_id"]
    last_watched_movies: TypedStatement<(), LastWatchedMovieRow>,
    #[sql = "SELECT show_id, MAX(timestamp) AS last_watched FROM watched_episodes GROUP BY show_id"]
    last_watched_shows: TypedStatement<(), LastWatchedShowRow>,

    // movies
    #[sql = "SELECT m.id, m.title, m.release_date, m.overview, m.tracked, m.auto_sync, m.last_synced_at, m.language, m.release_filters, ms.data AS settings_data"]
    #[sql = "FROM movies m LEFT JOIN movie_settings ms ON ms.movie_id = m.id ORDER BY m.title"]
    list_movies: TypedStatement<(), MovieRow>,
    #[sql = "SELECT m.id, m.title, m.release_date, m.overview, m.tracked, m.auto_sync, m.last_synced_at, m.language, m.release_filters, ms.data AS settings_data"]
    #[sql = "FROM movies m LEFT JOIN movie_settings ms ON ms.movie_id = m.id WHERE m.id = ?"]
    movie_by_id: TypedStatement<(MovieId,), MovieRow>,
    #[sql = "SELECT m.id, m.title, m.release_date, m.overview, m.tracked, m.auto_sync, m.last_synced_at, m.language, m.release_filters, ms.data AS settings_data"]
    #[sql = "FROM movies m"]
    #[sql = "JOIN movie_remotes r ON r.movie_id = m.id"]
    #[sql = "LEFT JOIN movie_settings ms ON ms.movie_id = m.id"]
    #[sql = "WHERE r.source = ? AND r.value = ?"]
    movie_by_remote: TypedStatement<(RemoteSource, RemoteValue), MovieRow>,
    #[sql = "SELECT id, slug, source, value, enabled, priority, sync_kinds FROM movie_remotes WHERE movie_id = ? ORDER BY priority, id"]
    list_movie_remotes: TypedStatement<(MovieId,), RemoteRow>,
    #[sql = "SELECT movie_id, id, slug, source, value, enabled, priority, sync_kinds FROM movie_remotes ORDER BY movie_id, priority, id"]
    list_all_movie_remotes: TypedStatement<(), AllMovieRemoteRow>,
    #[sql = "SELECT movie_id FROM movie_remotes WHERE source = ? AND value = ? LIMIT 1"]
    movie_id_by_remote: TypedStatement<(RemoteSource, RemoteValue), Option<MovieId>>,
    #[sql = "SELECT release_date FROM movies WHERE id = ?"]
    movie_released_by_id: TypedStatement<(MovieId,), Option<Timestamp>>,

    // watched
    #[sql = "SELECT we.id, we.timestamp, e.id AS episode_id, NULL AS movie_id, e.show_id"]
    #[sql = "FROM watched_episodes we"]
    #[sql = "JOIN episodes e ON e.show_id = we.show_id AND e.season = we.season AND e.episode = we.episode"]
    #[sql = "WHERE e.id = ?"]
    #[sql = "ORDER BY we.timestamp DESC"]
    list_watched_by_episode: TypedStatement<(EpisodeId,), WatchedRow>,
    #[sql = "SELECT id, timestamp, NULL AS episode_id, movie_id, NULL AS show_id"]
    #[sql = "FROM watched_movies WHERE movie_id = ? ORDER BY timestamp DESC"]
    list_watched_by_movie: TypedStatement<(MovieId,), WatchedRow>,
    #[sql = "SELECT we.id, we.timestamp, we.show_id, we.season, we.episode"]
    #[sql = "FROM watched_episodes we"]
    #[sql = "LEFT JOIN episodes e"]
    #[sql = "    ON e.show_id = we.show_id AND e.season = we.season AND e.episode = we.episode"]
    #[sql = "WHERE we.show_id = ? AND e.id IS NULL"]
    #[sql = "ORDER BY we.timestamp ASC"]
    list_orphaned_for_show: TypedStatement<(ShowId,), OrphanedWatchedRow>,
    // select episodes which have 0 watched by show and season.
    #[sql = "SELECT id, show_id, season, episode FROM episodes"]
    #[sql = "WHERE show_id = ? AND season = ?"]
    #[sql = "    AND NOT EXISTS ("]
    #[sql = "        SELECT 1 FROM watched_episodes we"]
    #[sql = "        WHERE we.show_id = episodes.show_id"]
    #[sql = "        AND we.season = episodes.season"]
    #[sql = "        AND we.episode = episodes.episode"]
    #[sql = "    )"]
    select_unwatched_by_show_season: TypedStatement<(ShowId, SeasonNumber), UnwatchedEpisodeRow>,

    // pending table management
    #[sql = "SELECT 1 FROM pending WHERE movie_id = ? LIMIT 1"]
    has_pending_movie: TypedStatement<(MovieId,), (i64,)>,
    #[sql = "SELECT 1 FROM pending WHERE show_id = ? LIMIT 1"]
    has_pending_episode_for_show: TypedStatement<(ShowId,), (i64,)>,
    #[sql = "SELECT e.aired, p.timestamp"]
    #[sql = "FROM pending p"]
    #[sql = "JOIN episodes e ON e.id = p.episode_id"]
    #[sql = "WHERE p.show_id = ?"]
    pending_episode_aired_for_show: TypedStatement<(ShowId,), PendingEpisodeAiredRow>,
    #[sql = "SELECT e.id, e.aired"]
    #[sql = "FROM episodes e"]
    #[sql = "WHERE e.show_id = ?1"]
    #[sql = "    AND e.aired IS NOT NULL"]
    #[sql = "    AND (?2 OR e.season <> 0)"]
    #[sql = "    AND NOT EXISTS ("]
    #[sql = "        SELECT 1 FROM watched_episodes we"]
    #[sql = "        WHERE we.show_id = e.show_id AND we.season = e.season AND we.episode = e.episode"]
    #[sql = "    )"]
    #[sql = "ORDER BY e.season, e.episode"]
    #[sql = "LIMIT 1"]
    next_pending_episode_for_show: TypedStatement<(ShowId, bool), NextEpisodeRow>,
    #[sql = "SELECT e.id, e.aired"]
    #[sql = "FROM episodes e"]
    #[sql = "WHERE e.show_id = ?"]
    #[sql = "    AND NOT EXISTS ("]
    #[sql = "        SELECT 1 FROM watched_episodes we"]
    #[sql = "        WHERE we.show_id = e.show_id AND we.season = e.season AND we.episode = e.episode"]
    #[sql = "    )"]
    #[sql = "ORDER BY e.season, e.episode"]
    #[sql = "LIMIT 1"]
    first_unwatched_episode_for_show: TypedStatement<(ShowId,), NextEpisodeRow>,
    #[sql = "SELECT m.id, m.release_filters"]
    #[sql = "FROM movies m"]
    #[sql = "WHERE m.tracked = 1"]
    #[sql = "    AND NOT EXISTS (SELECT 1 FROM watched_movies wm WHERE wm.movie_id = m.id)"]
    #[sql = "    AND NOT EXISTS (SELECT 1 FROM pending p WHERE p.movie_id = m.id)"]
    movie_pending_candidates: TypedStatement<(), MoviePendingCandidateRow>,
    #[sql = "SELECT 1 FROM watched_movies WHERE movie_id = ? LIMIT 1"]
    has_watched_movie: TypedStatement<(MovieId,), (i64,)>,
    #[sql = "SELECT episode_id, movie_id"]
    #[sql = "FROM pending"]
    #[sql = "WHERE timestamp <= ?"]
    #[sql = "ORDER BY timestamp DESC"]
    list_pending_before: TypedStatement<(Timestamp,), PendingBaseRow>,
    #[sql = "SELECT e.show_id, s.title AS show_title, e.season, e.episode, e.name AS episode_name, e.aired"]
    #[sql = "FROM episodes e"]
    #[sql = "JOIN shows s ON s.id = e.show_id"]
    #[sql = "WHERE e.id = ? AND s.tracked = 1"]
    pending_episode_detail: TypedStatement<(EpisodeId,), PendingEpisodeDetailRow>,
    #[sql = "SELECT title, release_date FROM movies WHERE id = ? AND tracked = 1"]
    pending_movie_detail: TypedStatement<(MovieId,), PendingMovieDetailRow>,
    #[sql = "SELECT i.source, i.path"]
    #[sql = "FROM show_images si JOIN images i ON i.id = si.image_id"]
    #[sql = "WHERE si.show_id = ? AND si.kind = ?"]
    image_for_show: TypedStatement<(ShowId, ImageKind), PendingImageRow>,
    #[sql = "SELECT i.source, i.path"]
    #[sql = "FROM movie_images mi JOIN images i ON i.id = mi.image_id"]
    #[sql = "WHERE mi.movie_id = ? AND mi.kind = ?"]
    image_for_movie: TypedStatement<(MovieId, ImageKind), PendingImageRow>,
    #[sql = "SELECT id FROM seasons WHERE show_id = ? AND season = ?"]
    season_id_for: TypedStatement<(ShowId, SeasonNumber), SeasonId>,
    #[sql = "SELECT e.id, e.aired FROM episodes e"]
    #[sql = "JOIN episodes c ON c.id = ?2"]
    #[sql = "WHERE e.show_id = ?1"]
    #[sql = "    AND (e.season > c.season OR (e.season = c.season AND e.episode > c.episode))"]
    #[sql = "ORDER BY e.season, e.episode"]
    #[sql = "LIMIT 1"]
    next_episode_after: TypedStatement<(ShowId, EpisodeId), (EpisodeId, Option<Timestamp>)>,

    // schedule: episodes airing in the next N days
    #[sql = "SELECT e.show_id, s.title AS show_title, e.season, e.episode, e.aired"]
    #[sql = "FROM episodes e"]
    #[sql = "JOIN shows s ON s.id = e.show_id"]
    #[sql = "WHERE s.tracked = 1"]
    #[sql = "    AND e.aired > ?"]
    #[sql = "    AND e.aired <= ?"]
    #[sql = "ORDER BY e.aired, s.title, e.season, e.episode"]
    list_schedule: TypedStatement<(Timestamp, Timestamp), ScheduleRow>,
    #[sql = "SELECT m.id, m.title, m.release_date"]
    #[sql = "FROM movies m"]
    #[sql = "WHERE m.tracked = 1"]
    #[sql = "    AND m.release_date > ?"]
    #[sql = "    AND m.release_date <= ?"]
    #[sql = "ORDER BY m.release_date, m.title"]
    list_schedule_movies: TypedStatement<(Timestamp, Timestamp), ScheduleMovieRow>,

    // all watched (for import dedup) see list_all_watched_episodes / list_all_watched_movies

    // config
    #[sql = "SELECT value FROM config WHERE key = ?"]
    get_config: TypedStatement<(String,), String>,

    // derived state (recomputed periodically)
    #[sql = "SELECT top_languages FROM state WHERE id = 0"]
    get_state_top_languages: TypedStatement<(), String>,
    #[sql = "SELECT language, show_settings.data AS settings_data"]
    #[sql = "FROM shows LEFT JOIN show_settings ON show_settings.show_id = shows.id"]
    list_show_languages: TypedStatement<(), LanguageRow>,
    #[sql = "SELECT m.language, ms.data AS settings_data"]
    #[sql = "FROM movies m LEFT JOIN movie_settings ms ON ms.movie_id = m.id"]
    list_movie_languages: TypedStatement<(), LanguageRow>,

    // stale-item queries
    #[sql = "SELECT shows.id, title, first_air, overview, tracked, auto_sync, last_synced_at, language, include_specials, air_date_filters, show_settings.data AS settings_data"]
    #[sql = "FROM shows LEFT JOIN show_settings ON show_settings.show_id = shows.id"]
    #[sql = "WHERE auto_sync = 1"]
    #[sql = "    AND (last_synced_at IS NULL OR last_synced_at < ?)"]
    #[sql = "ORDER BY last_synced_at IS NOT NULL, last_synced_at"]
    shows_needing_sync: TypedStatement<(Timestamp,), ShowRow>,
    #[sql = "SELECT m.id, m.title, m.release_date, m.overview, m.tracked, m.auto_sync, m.last_synced_at, m.language, m.release_filters, ms.data AS settings_data"]
    #[sql = "FROM movies m LEFT JOIN movie_settings ms ON ms.movie_id = m.id"]
    #[sql = "WHERE m.auto_sync = 1"]
    #[sql = "    AND (m.last_synced_at IS NULL OR m.last_synced_at < ?)"]
    #[sql = "ORDER BY m.last_synced_at IS NOT NULL, m.last_synced_at"]
    movies_needing_sync: TypedStatement<(Timestamp,), MovieRow>,

    // movie releases
    #[sql = "SELECT country, release_type, timestamp"]
    #[sql = "FROM movie_releases"]
    #[sql = "WHERE movie_id = ?"]
    #[sql = "ORDER BY timestamp, country, release_type"]
    list_movie_releases: TypedStatement<(MovieId,), MovieReleaseRow>,
    #[sql = "SELECT timestamp"]
    #[sql = "FROM movie_releases"]
    #[sql = "WHERE movie_id = ? AND release_type = ?"]
    #[sql = "ORDER BY timestamp"]
    movie_release_by_type: TypedStatement<(MovieId, ReleaseType), Timestamp>,

    // episode releases (air dates attributed to a source/country/network)
    #[sql = "SELECT er.episode_id, er.source, er.country, er.network, er.timestamp"]
    #[sql = "FROM episode_releases er"]
    #[sql = "JOIN episodes e ON e.id = er.episode_id"]
    #[sql = "WHERE e.show_id = ?"]
    list_episode_releases_for_show: TypedStatement<(ShowId,), EpisodeReleaseRow>,
}

#[derive(Statements)]
struct InnerWrite {
    #[sql(statements)]
    read: InnerRead,

    // shows
    #[sql = "INSERT INTO shows (id, title, first_air, overview, tracked)"]
    #[sql = "VALUES (?, ?, ?, ?, ?)"]
    insert_show: TypedStatement<(ShowId, String, Option<Timestamp>, String, bool), ()>,
    #[sql = "UPDATE shows"]
    #[sql = "SET title = ?, first_air = ?, overview = ?, tracked = ?"]
    #[sql = "WHERE id = ?"]
    update_show: TypedStatement<
        (
            Option<String>,
            Option<Timestamp>,
            Option<String>,
            bool,
            ShowId,
        ),
        (),
    >,
    #[sql = "DELETE FROM shows WHERE id = ?"]
    delete_show: TypedStatement<(ShowId,), ()>,
    #[sql = "UPDATE shows SET tracked = ? WHERE id = ?"]
    set_show_tracked: TypedStatement<(bool, ShowId), ()>,
    #[sql = "UPDATE shows SET auto_sync = ? WHERE id = ?"]
    set_show_auto_sync: TypedStatement<(bool, ShowId), ()>,
    // Per-show settings live in a single JSON blob; see api::ShowSettings.
    #[sql = "INSERT OR REPLACE INTO show_settings (show_id, data) VALUES (?, ?)"]
    upsert_show_settings: TypedStatement<(ShowId, String), ()>,

    // remotes (one table per owner; source is a numeric enum, value is dynamic)
    #[sql = "INSERT OR IGNORE INTO show_remotes (id, slug, show_id, source, value, enabled, priority, sync_kinds) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    insert_show_remote: TypedStatement<
        (
            RemoteId,
            Option<String>,
            ShowId,
            RemoteSource,
            RemoteValue,
            bool,
            i32,
            Option<api::SyncKindSet>,
        ),
        (),
    >,
    #[sql = "DELETE FROM show_remotes WHERE id = ?"]
    delete_show_remote: TypedStatement<(RemoteId,), ()>,
    #[sql = "UPDATE show_remotes SET slug = ?, source = ?, value = ? WHERE id = ?"]
    update_show_remote: TypedStatement<(Option<String>, RemoteSource, RemoteValue, RemoteId), ()>,
    #[sql = "UPDATE show_remotes SET enabled = ? WHERE id = ?"]
    set_show_remote_enabled: TypedStatement<(bool, RemoteId), ()>,
    #[sql = "UPDATE show_remotes SET priority = ? WHERE id = ?"]
    set_show_remote_priority: TypedStatement<(i32, RemoteId), ()>,
    #[sql = "UPDATE show_remotes SET sync_kinds = ? WHERE id = ?"]
    set_show_remote_sync_kinds: TypedStatement<(Option<api::SyncKindSet>, RemoteId), ()>,

    // images (shows and movies share one table)
    #[sql = "DELETE FROM images WHERE show_id = ?"]
    delete_show_images: TypedStatement<(ShowId,), ()>,
    #[sql = "INSERT INTO images (id, show_id, kind, source, path, width, height, rank) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(show_id, kind, path) WHERE show_id IS NOT NULL DO NOTHING"]
    insert_show_image: TypedStatement<
        (
            ImageId,
            ShowId,
            ImageKind,
            ImageSource,
            String,
            u32,
            u32,
            u32,
        ),
        (),
    >,
    #[sql = "INSERT INTO images (id, episode_id, kind, source, path, width, height) VALUES (?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(episode_id, kind, path) WHERE episode_id IS NOT NULL DO NOTHING"]
    insert_episode_image:
        TypedStatement<(ImageId, EpisodeId, ImageKind, ImageSource, String, u32, u32), ()>,
    #[sql = "DELETE FROM images WHERE episode_id IN (SELECT id FROM episodes WHERE show_id = ?)"]
    delete_episode_images_for_show: TypedStatement<(ShowId,), ()>,
    #[sql = "DELETE FROM images WHERE movie_id = ?"]
    delete_movie_images: TypedStatement<(MovieId,), ()>,
    #[sql = "INSERT INTO images (id, movie_id, kind, source, path, width, height, rank) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(movie_id, kind, path) WHERE movie_id IS NOT NULL DO NOTHING"]
    insert_movie_image: TypedStatement<
        (
            ImageId,
            MovieId,
            ImageKind,
            ImageSource,
            String,
            u32,
            u32,
            u32,
        ),
        (),
    >,
    #[sql = "DELETE FROM season_images WHERE season_id = ? AND kind = ?"]
    delete_season_image_selection: TypedStatement<(SeasonId, ImageKind), ()>,

    // selection tables
    #[sql = "INSERT OR REPLACE INTO show_images (show_id, kind, image_id) VALUES (?, ?, ?)"]
    set_show_image_selection: TypedStatement<(ShowId, ImageKind, ImageId), ()>,
    #[sql = "DELETE FROM show_images WHERE show_id = ? AND kind = ?"]
    delete_show_image_selection: TypedStatement<(ShowId, ImageKind), ()>,
    #[sql = "INSERT OR REPLACE INTO movie_images (movie_id, kind, image_id) VALUES (?, ?, ?)"]
    set_movie_image_selection: TypedStatement<(MovieId, ImageKind, ImageId), ()>,
    #[sql = "DELETE FROM movie_images WHERE movie_id = ? AND kind = ?"]
    delete_movie_image_selection: TypedStatement<(MovieId, ImageKind), ()>,
    #[sql = "INSERT OR REPLACE INTO episode_images (episode_id, kind, image_id) VALUES (?, ?, ?)"]
    set_episode_image_selection: TypedStatement<(EpisodeId, ImageKind, ImageId), ()>,
    #[sql = "DELETE FROM images WHERE season_id = ?"]
    delete_season_images: TypedStatement<(SeasonId,), ()>,
    #[sql = "INSERT INTO images (id, season_id, kind, source, path, width, height, rank) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(season_id, kind, path) WHERE season_id IS NOT NULL DO NOTHING"]
    insert_season_image: TypedStatement<
        (
            ImageId,
            SeasonId,
            ImageKind,
            ImageSource,
            String,
            u32,
            u32,
            u32,
        ),
        (),
    >,
    #[sql = "INSERT OR REPLACE INTO season_images (season_id, kind, image_id) VALUES (?, ?, ?)"]
    set_season_image_selection: TypedStatement<(SeasonId, ImageKind, ImageId), ()>,

    // seasons
    #[sql = "INSERT INTO seasons (id, show_id, season, air_date, name, overview)"]
    #[sql = "VALUES (?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(show_id, season) DO UPDATE SET"]
    #[sql = "    air_date  = excluded.air_date,"]
    #[sql = "    name      = excluded.name,"]
    #[sql = "    overview  = excluded.overview"]
    upsert_season: TypedStatement<
        (
            SeasonId,
            ShowId,
            SeasonNumber,
            Option<Timestamp>,
            Option<String>,
            Option<String>,
        ),
        (),
    >,
    #[sql = "DELETE FROM seasons WHERE show_id = ?1 AND season = ?2"]
    delete_season: TypedStatement<(ShowId, SeasonNumber), ()>,
    #[sql = "DELETE FROM episodes WHERE show_id = ?1 AND season = ?2"]
    delete_season_episodes: TypedStatement<(ShowId, SeasonNumber), ()>,
    #[sql = "DELETE FROM episodes WHERE show_id = ? AND season = ? AND episode = ?"]
    delete_episode_by_place: TypedStatement<(ShowId, SeasonNumber, u32), ()>,

    // episodes
    #[sql = "INSERT INTO episodes (id, show_id, season, episode, absolute_number, name, overview, aired)"]
    #[sql = "VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(show_id, season, episode) DO UPDATE SET"]
    #[sql = "    absolute_number = excluded.absolute_number,"]
    #[sql = "    name            = excluded.name,"]
    #[sql = "    overview        = excluded.overview,"]
    #[sql = "    aired           = excluded.aired"]
    upsert_episode: TypedStatement<
        (
            EpisodeId,
            ShowId,
            SeasonNumber,
            u32,
            Option<u32>,
            Option<String>,
            Option<String>,
            Option<Timestamp>,
        ),
        (),
    >,
    #[sql = "DELETE FROM episode_remotes WHERE episode_id = ?"]
    delete_episode_remotes: TypedStatement<(EpisodeId,), ()>,
    #[sql = "INSERT INTO episode_remotes (id, episode_id, source, value) VALUES (?, ?, ?, ?)"]
    insert_episode_remote: TypedStatement<(RemoteId, EpisodeId, RemoteSource, RemoteValue), ()>,
    #[sql = "UPDATE episodes SET remote_id = ? WHERE id = ?"]
    set_episode_remote: TypedStatement<(RemoteId, EpisodeId), ()>,
    #[sql = "UPDATE episodes SET aired = ? WHERE id = ?"]
    set_episode_aired_by_id: TypedStatement<(Option<Timestamp>, EpisodeId), ()>,
    #[sql = "UPDATE episodes SET aired = NULL WHERE show_id = ?"]
    clear_episode_aired_for_show: TypedStatement<(ShowId,), ()>,
    #[sql = "INSERT INTO episode_releases (episode_id, source, country, network, timestamp)"]
    #[sql = "VALUES (?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(episode_id, source, country, network)"]
    #[sql = "    DO UPDATE SET timestamp = excluded.timestamp"]
    upsert_episode_release:
        TypedStatement<(EpisodeId, RemoteSource, String, String, Timestamp), ()>,

    // movies
    #[sql = "INSERT INTO movies (id, title, release_date, overview, tracked)"]
    #[sql = "VALUES (?, ?, ?, ?, ?)"]
    insert_movie: TypedStatement<(MovieId, String, Option<Timestamp>, String, bool), ()>,
    #[sql = "UPDATE movies SET tracked = ? WHERE id = ?"]
    set_movie_tracked: TypedStatement<(bool, MovieId), ()>,
    #[sql = "UPDATE movies SET auto_sync = ? WHERE id = ?"]
    set_movie_auto_sync: TypedStatement<(bool, MovieId), ()>,
    // Per-movie settings live in a single JSON blob; see api::MovieSettings.
    #[sql = "INSERT OR REPLACE INTO movie_settings (movie_id, data) VALUES (?, ?)"]
    upsert_movie_settings: TypedStatement<(MovieId, String), ()>,
    #[sql = "UPDATE movies"]
    #[sql = "SET title = ?, release_date = ?, overview = ?"]
    #[sql = "WHERE id = ?"]
    update_movie: TypedStatement<(Option<String>, Option<Timestamp>, Option<String>, MovieId), ()>,
    #[sql = "UPDATE movies SET release_date = ? WHERE id = ?"]
    set_movie_release_date: TypedStatement<(Option<Timestamp>, MovieId), ()>,
    #[sql = "DELETE FROM movies WHERE id = ?"]
    delete_movie: TypedStatement<(MovieId,), ()>,
    #[sql = "INSERT OR IGNORE INTO movie_remotes (id, slug, movie_id, source, value, enabled, priority, sync_kinds) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    insert_movie_remote: TypedStatement<
        (
            RemoteId,
            Option<String>,
            MovieId,
            RemoteSource,
            RemoteValue,
            bool,
            i32,
            Option<api::SyncKindSet>,
        ),
        (),
    >,
    #[sql = "DELETE FROM movie_remotes WHERE id = ?"]
    delete_movie_remote: TypedStatement<(RemoteId,), ()>,
    #[sql = "UPDATE movie_remotes SET slug = ?, source = ?, value = ? WHERE id = ?"]
    update_movie_remote: TypedStatement<(Option<String>, RemoteSource, RemoteValue, RemoteId), ()>,
    #[sql = "UPDATE movie_remotes SET enabled = ? WHERE id = ?"]
    set_movie_remote_enabled: TypedStatement<(bool, RemoteId), ()>,
    #[sql = "UPDATE movie_remotes SET priority = ? WHERE id = ?"]
    set_movie_remote_priority: TypedStatement<(i32, RemoteId), ()>,
    #[sql = "UPDATE movie_remotes SET sync_kinds = ? WHERE id = ?"]
    set_movie_remote_sync_kinds: TypedStatement<(Option<api::SyncKindSet>, RemoteId), ()>,

    // watched
    #[sql = "INSERT OR IGNORE INTO watched_episodes (id, timestamp, show_id, season, episode)"]
    #[sql = "VALUES (?, ?, ?, ?, ?)"]
    insert_watched_episode: TypedStatement<(WatchedId, Timestamp, ShowId, SeasonNumber, u32), ()>,
    #[sql = "INSERT OR IGNORE INTO watched_movies (id, timestamp, movie_id)"]
    #[sql = "VALUES (?, ?, ?)"]
    insert_watched_movie: TypedStatement<(WatchedId, Timestamp, MovieId), ()>,
    #[sql = "DELETE FROM watched_episodes WHERE id = ?"]
    delete_watched_episode: TypedStatement<(WatchedId,), ()>,
    #[sql = "DELETE FROM watched_movies WHERE id = ?"]
    delete_watched_movie: TypedStatement<(WatchedId,), ()>,
    #[sql = "UPDATE watched_episodes SET season = ?, episode = ? WHERE id = ?"]
    move_watched_episode: TypedStatement<(SeasonNumber, u32, WatchedId), ()>,

    // pending table management
    #[sql = "INSERT INTO pending (id, timestamp, show_id, episode_id) VALUES (?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(show_id) WHERE show_id IS NOT NULL"]
    #[sql = "    DO UPDATE SET episode_id = excluded.episode_id, timestamp = excluded.timestamp"]
    upsert_pending_episode: TypedStatement<(PendingId, Timestamp, ShowId, EpisodeId), ()>,
    #[sql = "INSERT INTO pending (id, timestamp, movie_id) VALUES (?, ?, ?)"]
    #[sql = "ON CONFLICT(movie_id) WHERE movie_id IS NOT NULL"]
    #[sql = "    DO UPDATE SET timestamp = excluded.timestamp"]
    upsert_pending_movie: TypedStatement<(PendingId, Timestamp, MovieId), ()>,
    #[sql = "UPDATE pending SET timestamp = ? WHERE show_id = ?"]
    update_pending_episode_timestamp: TypedStatement<(Timestamp, ShowId), ()>,
    #[sql = "DELETE FROM pending WHERE show_id = ?"]
    delete_pending_episode: TypedStatement<(ShowId,), ()>,
    #[sql = "DELETE FROM pending WHERE movie_id = ?"]
    delete_pending_movie: TypedStatement<(MovieId,), ()>,

    // config
    #[sql = "INSERT INTO config (key, value) VALUES (?, ?)"]
    #[sql = "ON CONFLICT (key) DO UPDATE SET value = excluded.value"]
    set_config: TypedStatement<(String, String), ()>,
    #[sql = "DELETE FROM config WHERE key = ?"]
    delete_config: TypedStatement<(String,), ()>,

    // derived state
    #[sql = "UPDATE state SET top_languages = ? WHERE id = 0"]
    set_state_top_languages: TypedStatement<(String,), ()>,

    // movie releases
    #[sql = "INSERT INTO movie_releases (id, movie_id, country, release_type, timestamp)"]
    #[sql = "VALUES (?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(movie_id, country, release_type)"]
    #[sql = "    DO UPDATE SET timestamp = excluded.timestamp"]
    upsert_movie_release:
        TypedStatement<(MovieReleaseId, MovieId, String, ReleaseType, Timestamp), ()>,

    // last_synced_at stamping
    #[sql = "UPDATE shows SET last_synced_at = ? WHERE id = ?"]
    set_show_synced_at: TypedStatement<(Timestamp, ShowId), ()>,
    #[sql = "UPDATE movies SET last_synced_at = ? WHERE id = ?"]
    set_movie_synced_at: TypedStatement<(Timestamp, MovieId), ()>,
}

impl Deref for InnerWrite {
    type Target = InnerRead;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.read
    }
}

impl DerefMut for InnerWrite {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.read
    }
}

impl InnerRead {
    fn get_config(&mut self, key: &str) -> Result<Option<String>> {
        Ok(self.get_config.bind((key,))?.first()?)
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

    fn image_for_show(&mut self, show_id: ShowId, kind: ImageKind) -> Result<Option<api::Image>> {
        let poster_row = self.image_for_show.bind((show_id, kind))?.first()?;
        Ok(poster_row.map(|p| api::Image::new(p.source, &p.path)))
    }

    fn image_for_movie(
        &mut self,
        movie_id: MovieId,
        kind: ImageKind,
    ) -> Result<Option<api::Image>> {
        let poster_row = self.image_for_movie.bind((movie_id, kind))?.first()?;
        Ok(poster_row.map(|p| api::Image::new(p.source, &p.path)))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum OpenMode {
    /// Full synchronization safe for the server.
    Normal,
    /// No journaling or fsync fast for bulk import; not crash-safe.
    Bulk,
}

pub(crate) struct Database {
    inner: Arc<Pool<InnerRead, InnerWrite>>,
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

        {
            let c = OpenOptions::new()
                .extended_result_codes()
                .read_write()
                .create()
                .no_mutex()
                .open(path.as_os_str())
                .with_context(|| anyhow!("Opening database at {}", path.display()))?;

            do_migrations(&c).context("Running migrations")?;
        }

        let mut options = OpenOptions::new();
        options.no_mutex();

        let builder = PoolBuilder::new(options, 16)
            .with_write_setup(move |c| ensure_mode(c, mode))
            .with_read_setup(move |c| ensure_mode(c, mode));

        Ok(Self {
            inner: Arc::new(builder.open(path)?),
        })
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn create_show(
        &self,
        id: ShowId,
        title: &str,
        first_air: Option<Timestamp>,
        overview: &str,
    ) -> Result<()> {
        let title = title.to_owned();
        let overview = overview.to_owned();
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_show
                .execute((id, &title[..], first_air.as_ref(), &overview[..], true))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn show_id_by_remote(&self, remote: &Remote) -> Result<Option<ShowId>> {
        let remote = remote.clone();
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            s.show_id_by_remote
                .bind((remote.source(), remote.value()))?
                .first()
        });

        Ok(result.await??)
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn add_show_remote(
        &self,
        show_id: ShowId,
        slug: Option<&str>,
        remote: &Remote,
    ) -> Result<()> {
        let remote = remote.clone();
        let slug = slug.map(str::to_owned);
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_show_remote.execute((
                RemoteId::random(),
                slug,
                show_id,
                remote.source(),
                remote.value(),
                true,
                default_remote_priority(*remote.source()),
                // NULL = inherit the global per-source sync-kinds default.
                None::<api::SyncKindSet>,
            ))
        });

        result.await??;
        Ok(())
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn remove_show_remote(&self, remote_id: RemoteId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || s.delete_show_remote.execute((remote_id,)));

        result.await??;
        Ok(())
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn update_show_remote(
        &self,
        remote_id: RemoteId,
        slug: Option<&str>,
        remote: &Remote,
    ) -> Result<()> {
        let slug = slug.map(str::to_owned);
        let remote = remote.clone();
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.update_show_remote.execute((
                slug.as_deref(),
                remote.source(),
                remote.value(),
                remote_id,
            ))
        });

        result.await??;
        Ok(())
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn shows(&self) -> Result<Vec<api::Show>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out: Vec<api::Show> = Vec::new();
            let mut id_to_idx: HashMap<ShowId, usize> = HashMap::new();

            let mut stmt = s.list_shows.query()?;

            while let Some(row) = stmt.next()? {
                let idx = out.len();
                id_to_idx.insert(row.id, idx);

                out.push(show_from_row(row));
            }

            stmt.reset()?;

            for show in &mut out {
                show.poster = s.image_for_show(show.id, ImageKind::Poster)?;
                show.banner = s.image_for_show(show.id, ImageKind::Banner)?;
            }

            let mut stmt = s.list_all_show_remotes.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(o) = id_to_idx.get(&r.show_id).and_then(|&i| out.get_mut(i)) {
                    o.remotes.push(api::RemoteEntry {
                        id: r.id,
                        slug: r.slug,
                        remote: Remote::new(r.source, r.value),
                        enabled: r.enabled,
                        priority: r.priority,
                        sync_kinds: r.sync_kinds,
                    });
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_show_images.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(o) = id_to_idx.get(&r.show_id).and_then(|&i| out.get_mut(i)) {
                    o.images.push(show_image_from_row(r));
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_show_image_selections.query()?;

            while let Some(row) = stmt.next()? {
                if let Some(o) = id_to_idx.get(&row.show_id).and_then(|&i| out.get_mut(i)) {
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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn show_by_id(&self, id: ShowId) -> Result<Option<api::Show>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let Some(r) = s.show_by_id.bind((id,))?.first()? else {
                return Ok(None);
            };

            let mut show = show_from_row(r);

            let mut stmt = s.list_show_remotes.bind((id,))?;

            while let Some(r) = stmt.next()? {
                show.remotes.push(api::RemoteEntry {
                    id: r.id,
                    slug: r.slug,
                    remote: Remote::new(r.source, r.value),
                    enabled: r.enabled,
                    priority: r.priority,
                    sync_kinds: r.sync_kinds,
                });
            }

            stmt.reset()?;

            let mut stmt = s.list_show_images.bind((id,))?;

            while let Some(row) = stmt.next()? {
                show.images.push(image_from_row(row));
            }

            stmt.reset()?;

            let mut stmt = s.list_show_image_selections.bind((id,))?;

            while let Some(sel) = stmt.next()? {
                apply_image_selection(&mut show, sel);
            }

            stmt.reset()?;

            show.poster = s.image_for_show(show.id, ImageKind::Poster)?;
            show.banner = s.image_for_show(show.id, ImageKind::Banner)?;
            Ok(Some(show))
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn update_show(
        &self,
        id: ShowId,
        title: Option<&str>,
        first_air: Option<Timestamp>,
        overview: Option<&str>,
        tracked: bool,
    ) -> Result<()> {
        let title = title.map(str::to_owned);
        let overview = overview.map(str::to_owned);

        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.update_show.execute((
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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn delete_show(&self, id: ShowId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_show.execute((id,))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_tracked(&self, id: ShowId, tracked: bool) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_show_tracked.execute((tracked, id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_auto_sync(&self, id: ShowId, auto_sync: bool) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_show_auto_sync.execute((auto_sync, id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_remote_enabled(
        &self,
        remote_id: RemoteId,
        enabled: bool,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_show_remote_enabled.execute((enabled, remote_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_remote_sync_kinds(
        &self,
        remote_id: RemoteId,
        sync_kinds: Option<api::SyncKindSet>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_show_remote_sync_kinds
                .execute((sync_kinds, remote_id))?;
            Ok(())
        });

        result.await?
    }

    /// Set remote priority to match the given order (first = highest priority).
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn reorder_show_remotes(&self, remote_ids: Vec<RemoteId>) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            for (idx, id) in remote_ids.iter().enumerate() {
                s.set_show_remote_priority.execute((idx as i32, *id))?;
            }
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_language(
        &self,
        id: ShowId,
        language: Option<String>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let mut settings = current_show_settings(&mut s, id)?;
            settings.language = language;
            s.upsert_show_settings
                .execute((id, api::encode_show_settings(&settings)))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_include_specials(
        &self,
        id: ShowId,
        include_specials: Option<bool>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let mut settings = current_show_settings(&mut s, id)?;
            settings.include_specials = include_specials;
            s.upsert_show_settings
                .execute((id, api::encode_show_settings(&settings)))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn upsert_season(
        &self,
        show_id: ShowId,
        number: SeasonNumber,
        air_date: Option<Timestamp>,
        name: Option<&str>,
        overview: Option<&str>,
    ) -> Result<SeasonId> {
        let name = name.map(str::to_owned);
        let overview = overview.map(str::to_owned);
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.upsert_season.execute((
                SeasonId::random(),
                show_id,
                number,
                air_date.as_ref(),
                name.as_deref(),
                overview.as_deref(),
            ))?;
            let id = s
                .season_id_for
                .bind((show_id, number))?
                .first()?
                .context("Season missing after upsert")?;
            Ok(id)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn upsert_season_image(
        &self,
        id: ImageId,
        season_id: SeasonId,
        kind: ImageKind,
        image: &api::Image,
    ) -> Result<()> {
        let image = image.clone();
        let mut s = self.inner.clone().exclusive().await?;

        spawn_blocking(move || {
            s.insert_season_image.execute((
                id,
                season_id,
                kind,
                image.key().source(),
                image.key().path(),
                image.width(),
                image.height(),
                0u32,
            ))?;
            Ok(())
        })
        .await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_season_image_selection(
        &self,
        season_id: SeasonId,
        kind: ImageKind,
        image_id: ImageId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        spawn_blocking(move || {
            s.set_season_image_selection
                .execute((season_id, kind, image_id))?;
            Ok(())
        })
        .await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn clear_season_images(&self, season_id: SeasonId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        spawn_blocking(move || {
            s.delete_season_images.execute((season_id,))?;
            Ok(())
        })
        .await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn seasons(&self, show_id: ShowId) -> Result<Vec<api::Season>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_seasons.bind((show_id,))?;

            while let Some(r) = stmt.next()? {
                out.push(season_from_row(r));
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn prune_seasons(
        &self,
        show_id: ShowId,
        kept: &HashSet<SeasonNumber>,
    ) -> Result<Vec<SeasonNumber>> {
        let existing = self.seasons(show_id).await?;
        let mut removed = Vec::new();

        for season in existing {
            if kept.contains(&season.season) {
                continue;
            }

            let n = season.season;
            let mut s = self.inner.clone().exclusive().await?;

            let result = spawn_blocking(move || {
                s.delete_season_episodes.execute((show_id, n))?;
                s.delete_season.execute((show_id, n))?;
                Ok::<_, anyhow::Error>(())
            });

            result.await??;
            removed.push(season.season);
        }

        Ok(removed)
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn prune_season_episodes(
        &self,
        show_id: ShowId,
        season: SeasonNumber,
        kept: &HashSet<u32>,
    ) -> Result<()> {
        let kept = kept.clone();
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let mut to_delete = Vec::new();

            {
                let mut stmt = s.episode_numbers_for_season.bind((show_id, season))?;

                while let Some(number) = stmt.next()? {
                    if !kept.contains(&number) {
                        to_delete.push(number);
                    }
                }
            }

            for number in to_delete {
                s.delete_episode_by_place
                    .execute((show_id, season, number))?;
            }

            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn upsert_episode(
        &self,
        id: EpisodeId,
        show_id: ShowId,
        season: SeasonNumber,
        number: u32,
        absolute_number: Option<u32>,
        name: Option<&str>,
        overview: Option<&str>,
        aired: Option<Timestamp>,
        remote: Option<&Remote>,
    ) -> Result<()> {
        let name = name.map(str::to_owned);
        let overview = overview.map(str::to_owned);
        let remote = remote.cloned();
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.upsert_episode.execute((
                id,
                show_id,
                season,
                number,
                absolute_number,
                name.as_deref(),
                overview.as_deref(),
                aired.as_ref(),
            ))?;

            // Reset the episode's single remote to the synced value. Deleting
            // the old row clears episodes.remote_id via ON DELETE SET NULL.
            s.delete_episode_remotes.execute((id,))?;

            if let Some(remote) = &remote {
                let remote_id = RemoteId::random();
                s.insert_episode_remote.execute((
                    remote_id,
                    id,
                    remote.source(),
                    remote.value(),
                ))?;
                s.set_episode_remote.execute((remote_id, id))?;
            }

            Ok(())
        });

        result.await?
    }

    /// Map of `(season, number)` to the existing episode id for a show, so a
    /// re-sync can reuse stable ids rather than allocating new ones.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn episode_ids(
        &self,
        show_id: ShowId,
    ) -> Result<HashMap<(SeasonNumber, u32), EpisodeId>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = HashMap::new();

            let mut stmt = s.list_episode_ids_for_show.bind((show_id,))?;

            while let Some(r) = stmt.next()? {
                out.insert((r.season, r.number), r.id);
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn episodes(
        &self,
        show_id: ShowId,
        season: SeasonNumber,
    ) -> Result<Vec<api::Episode>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut idx_by_id = HashMap::new();

            let mut stmt = s.list_episodes.bind((show_id, season))?;

            while let Some(r) = stmt.next()? {
                idx_by_id.insert(r.id, out.len());
                out.push(episode_from_row(r));
            }

            stmt.reset()?;

            let mut stmt =
                s.list_season_episode_screenshots
                    .bind((ImageKind::Screenshot, show_id, season))?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = idx_by_id.get(&r.episode_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.screenshot = Some(Image::new_with_dims(r.source, &r.path, r.width, r.height));
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_season_episode_remotes.bind((show_id, season))?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = idx_by_id.get(&r.episode_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.remote_id = Some(Remote::new(r.source, r.value));
                }
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn mark_watched_remaining(
        &self,
        show_id: ShowId,
        season: SeasonNumber,
        mark_time: MarkTime,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let mut unwatched = Vec::new();

            let mut stmt = s.select_unwatched_by_show_season.bind((show_id, season))?;

            while let Some(r) = stmt.next()? {
                unwatched.push(r);
            }

            stmt.reset()?;

            for r in unwatched {
                let timestamp = s.episode_mark_time(r.id, mark_time, now)?;
                s.insert_watched_episode.execute((
                    WatchedId::random(),
                    timestamp,
                    r.show_id,
                    r.season,
                    r.number,
                ))?;
            }

            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn episodes_watched(
        &self,
        show_id: ShowId,
    ) -> Result<Vec<api::WatchedEpisode>> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_episodes_watched.bind((show_id,))?;

            while let Some(r) = stmt.next()? {
                out.push(watched_episode_from_row(r));
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn episode_aired_by_id(&self, id: EpisodeId) -> Result<Option<Timestamp>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let stmt = s.episode_aired_by_id.bind((id,))?;
            Ok(stmt.first()?.flatten())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn upsert_episode_release(
        &self,
        episode_id: EpisodeId,
        source: RemoteSource,
        country: &str,
        network: &str,
        timestamp: Timestamp,
    ) -> Result<()> {
        let country = country.to_owned();
        let network = network.to_owned();
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.upsert_episode_release
                .execute((episode_id, source, country, network, timestamp))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_air_date_filters(
        &self,
        id: ShowId,
        air_date_filters: Option<Vec<api::AirDateFilter>>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let mut settings = current_show_settings(&mut s, id)?;
            settings.air_date_filters = air_date_filters;
            s.upsert_show_settings
                .execute((id, api::encode_show_settings(&settings)))?;
            Ok(())
        });

        result.await?
    }

    /// Recompute each episode's effective `aired` from its stored releases, using
    /// the show's air-date-eligible remote priority and air-date filters (falling
    /// back to `default_filters`). An episode whose releases all come from excluded
    /// sources (or are filtered out) has its date cleared; when no source is
    /// eligible at all (air dates excluded from every remote) every episode's date
    /// is cleared. Episodes with no stored release are left untouched.
    #[tracing::instrument(skip(self, default_filters), ret(level = "trace"))]
    pub(crate) async fn recompute_episode_aired_for_show(
        &self,
        show_id: ShowId,
        default_filters: Vec<api::AirDateFilter>,
    ) -> Result<()> {
        let Some(show) = self.show_by_id(show_id).await? else {
            return Ok(());
        };

        let config = self.load_config().await?;
        let priority = api::air_date_sources_by_priority(&show.remotes, &config);
        let filters = show.air_date_filters.unwrap_or(default_filters);

        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            // No eligible source: drop every air date so excluded dates are no
            // longer shown (stale releases may remain stored, ready to be restored
            // if a source is re-enabled and the show recomputed).
            if priority.is_empty() {
                s.clear_episode_aired_for_show.execute((show_id,))?;
                return Ok(());
            }

            let mut by_episode: HashMap<EpisodeId, Vec<api::EpisodeRelease>> = HashMap::new();

            {
                let mut stmt = s.list_episode_releases_for_show.bind((show_id,))?;

                while let Some(r) = stmt.next()? {
                    by_episode
                        .entry(r.episode_id)
                        .or_default()
                        .push(api::EpisodeRelease {
                            source: r.source,
                            country: r.country,
                            network: r.network,
                            timestamp: r.timestamp,
                        });
                }
            }

            // Each episode with stored releases is set to its effective date, or
            // cleared when none of its releases qualify under the current priority
            // and filters.
            for (episode_id, releases) in by_episode {
                let aired = api::effective_aired(&releases, &priority, &filters);
                s.set_episode_aired_by_id.execute((aired, episode_id))?;
            }

            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
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
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_movie
                .execute((id, &title[..], release_date, &overview[..], tracked))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_id_by_remote(&self, remote: &Remote) -> Result<Option<MovieId>> {
        let remote = remote.clone();
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            Ok(s.movie_id_by_remote
                .bind((remote.source(), remote.value()))?
                .first()?
                .flatten())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn add_movie_remote(
        &self,
        movie_id: MovieId,
        slug: Option<&str>,
        remote: &Remote,
    ) -> Result<()> {
        let remote = remote.clone();
        let slug = slug.map(str::to_owned);
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_movie_remote.execute((
                RemoteId::random(),
                slug,
                movie_id,
                remote.source(),
                remote.value(),
                true,
                default_remote_priority(*remote.source()),
                // NULL = inherit the global per-source sync-kinds default.
                None::<api::SyncKindSet>,
            ))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn remove_movie_remote(&self, remote_id: RemoteId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_movie_remote.execute((remote_id,))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn update_movie_remote(
        &self,
        remote_id: RemoteId,
        slug: Option<&str>,
        remote: &Remote,
    ) -> Result<()> {
        let slug = slug.map(str::to_owned);
        let remote = remote.clone();
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.update_movie_remote.execute((
                slug.as_deref(),
                remote.source(),
                remote.value(),
                remote_id,
            ))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    /// List every show and movie as slim [`api::MediaItem`]s, each tagged with
    /// its [`api::MediaKind`]. The frontend filters and sorts client-side.
    pub(crate) async fn media_items(&self) -> Result<Vec<api::MediaItem>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out: Vec<api::MediaItem> = Vec::new();

            // Movies. Keyed separately from shows so the raw ids can't collide.
            {
                let base = out.len();
                let mut id_to_idx: HashMap<u64, usize> = HashMap::new();

                let mut stmt = s.list_movie_items.query()?;

                while let Some(row) = stmt.next()? {
                    let item = media_item_from_row(row, api::MediaKind::Movies);
                    id_to_idx.insert(item.id, out.len());
                    out.push(item);
                }

                stmt.reset()?;

                for item in &mut out[base..] {
                    let id = MovieId::new(item.id);
                    item.poster = s.image_for_movie(id, ImageKind::Poster)?;
                    item.banner = s.image_for_movie(id, ImageKind::Banner)?;
                    item.backdrop = s.image_for_movie(id, ImageKind::Backdrop)?;
                }

                {
                    let mut stmt = s.last_watched_movies.query()?;

                    while let Some(row) = stmt.next()? {
                        if let Some(o) = id_to_idx
                            .get(&row.movie_id.get())
                            .and_then(|&i| out.get_mut(i))
                        {
                            o.last_watched_at = Some(row.last_watched);
                        }
                    }
                }

                let mut stmt = s.list_all_movie_remotes.query()?;

                while let Some(r) = stmt.next()? {
                    if let Some(o) = id_to_idx
                        .get(&r.movie_id.get())
                        .and_then(|&i| out.get_mut(i))
                    {
                        o.remotes.push(api::RemoteEntry {
                            id: r.id,
                            slug: r.slug,
                            remote: Remote::new(r.source, r.value),
                            enabled: r.enabled,
                            priority: r.priority,
                            sync_kinds: r.sync_kinds,
                        });
                    }
                }
            }

            // Shows.
            {
                let base = out.len();
                let mut id_to_idx: HashMap<u64, usize> = HashMap::new();

                let mut stmt = s.list_show_items.query()?;

                while let Some(row) = stmt.next()? {
                    let item = media_item_from_row(row, api::MediaKind::Shows);
                    id_to_idx.insert(item.id, out.len());
                    out.push(item);
                }

                stmt.reset()?;

                for item in &mut out[base..] {
                    let id = ShowId::new(item.id);
                    item.poster = s.image_for_show(id, ImageKind::Poster)?;
                    item.banner = s.image_for_show(id, ImageKind::Banner)?;
                    item.backdrop = s.image_for_show(id, ImageKind::Backdrop)?;
                }

                {
                    let mut stmt = s.last_watched_shows.query()?;

                    while let Some(row) = stmt.next()? {
                        if let Some(o) = id_to_idx
                            .get(&row.show_id.get())
                            .and_then(|&i| out.get_mut(i))
                        {
                            o.last_watched_at = Some(row.last_watched);
                        }
                    }
                }

                let mut stmt = s.list_all_show_remotes.query()?;

                while let Some(r) = stmt.next()? {
                    if let Some(o) = id_to_idx
                        .get(&r.show_id.get())
                        .and_then(|&i| out.get_mut(i))
                    {
                        o.remotes.push(api::RemoteEntry {
                            id: r.id,
                            slug: r.slug,
                            remote: Remote::new(r.source, r.value),
                            enabled: r.enabled,
                            priority: r.priority,
                            sync_kinds: r.sync_kinds,
                        });
                    }
                }
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movies(&self) -> Result<Vec<api::Movie>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out: Vec<api::Movie> = Vec::new();
            let mut id_to_idx: HashMap<MovieId, usize> = HashMap::new();

            let mut stmt = s.list_movies.query()?;

            while let Some(row) = stmt.next()? {
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

            while let Some(r) = stmt.next()? {
                if let Some(&index) = id_to_idx.get(&r.movie_id)
                    && let Some(o) = out.get_mut(index)
                {
                    o.remotes.push(api::RemoteEntry {
                        id: r.id,
                        slug: r.slug,
                        remote: Remote::new(r.source, r.value),
                        enabled: r.enabled,
                        priority: r.priority,
                        sync_kinds: r.sync_kinds,
                    });
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_movie_images.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(o) = id_to_idx.get(&r.movie_id).and_then(|&i| out.get_mut(i)) {
                    o.images.push(movie_image_from_row(r));
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_movie_image_selections.query()?;

            while let Some(row) = stmt.next()? {
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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_by_id(&self, id: MovieId) -> Result<Option<api::Movie>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let Some(r) = s.movie_by_id.bind((id,))?.first()? else {
                return Ok(None);
            };

            let movie_id = r.id;
            let mut movie = movie_from_row(r);

            let mut stmt = s.list_movie_remotes.bind((movie_id,))?;

            while let Some(r) = stmt.next()? {
                movie.remotes.push(api::RemoteEntry {
                    id: r.id,
                    slug: r.slug,
                    remote: Remote::new(r.source, r.value),
                    enabled: r.enabled,
                    priority: r.priority,
                    sync_kinds: r.sync_kinds,
                });
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_images.bind((movie_id,))?;

            while let Some(r) = stmt.next()? {
                movie.images.push(image_from_row(r));
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_image_selections.bind((movie_id,))?;

            while let Some(sel) = stmt.next()? {
                apply_movie_image_selection(&mut movie, sel);
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_releases.bind((movie_id,))?;

            while let Some(r) = stmt.next()? {
                movie.releases.push(api::MovieRelease {
                    country: r.country,
                    release_type: r.release_type,
                    timestamp: r.timestamp,
                });
            }

            stmt.reset()?;

            movie.pending = s.has_pending_movie.bind((movie_id,))?.first()?.is_some();

            movie.poster = s.image_for_movie(movie_id, ImageKind::Poster)?;
            movie.banner = s.image_for_movie(movie_id, ImageKind::Banner)?;

            Ok(Some(movie))
        });

        result.await?
    }

    /// Earliest digital or physical release for a movie, used to date a pending
    /// slot to when the movie became available to watch.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn earliest_movie_release(&self, id: MovieId) -> Result<Option<Timestamp>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut earliest: Option<Timestamp> = None;

            for ty in [ReleaseType::Digital, ReleaseType::Physical] {
                if let Some(ts) = s.movie_release_by_type.bind((id, ty))?.first()? {
                    earliest = Some(earliest.map_or(ts, |e| e.min(ts)));
                }
            }

            Ok(earliest)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn shows_by_remote_id(&self, remote: &Remote) -> Result<Option<api::Show>> {
        let remote = remote.clone();
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let Some(row) = s
                .shows_by_remote
                .bind((remote.source(), remote.value()))?
                .first()?
            else {
                return Ok(None);
            };

            let show_id = row.id;
            let mut show = show_from_row(row);

            let mut stmt = s.list_show_remotes.bind((show_id,))?;

            while let Some(r) = stmt.next()? {
                show.remotes.push(api::RemoteEntry {
                    id: r.id,
                    slug: r.slug,
                    remote: Remote::new(r.source, r.value),
                    enabled: r.enabled,
                    priority: r.priority,
                    sync_kinds: r.sync_kinds,
                });
            }

            stmt.reset()?;

            let mut stmt = s.list_show_images.bind((show_id,))?;

            while let Some(r) = stmt.next()? {
                show.images.push(image_from_row(r));
            }

            stmt.reset()?;

            let mut stmt = s.list_show_image_selections.bind((show_id,))?;

            while let Some(sel) = stmt.next()? {
                apply_image_selection(&mut show, sel);
            }

            stmt.reset()?;

            show.poster = s.image_for_show(show_id, ImageKind::Poster)?;
            show.banner = s.image_for_show(show_id, ImageKind::Banner)?;
            Ok(Some(show))
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_by_remote_id(&self, remote: &Remote) -> Result<Option<api::Movie>> {
        let remote = remote.clone();
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let Some(row) = s
                .movie_by_remote
                .bind((remote.source(), remote.value()))?
                .first()?
            else {
                return Ok(None);
            };

            let movie_id = row.id;
            let mut movie = movie_from_row(row);

            let mut stmt = s.list_movie_remotes.bind((movie_id,))?;

            while let Some(r) = stmt.next()? {
                movie.remotes.push(api::RemoteEntry {
                    id: r.id,
                    slug: r.slug,
                    remote: Remote::new(r.source, r.value),
                    enabled: r.enabled,
                    priority: r.priority,
                    sync_kinds: r.sync_kinds,
                });
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_images.bind((movie_id,))?;

            while let Some(r) = stmt.next()? {
                movie.images.push(image_from_row(r));
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_image_selections.bind((movie_id,))?;

            while let Some(sel) = stmt.next()? {
                apply_movie_image_selection(&mut movie, sel);
            }

            stmt.reset()?;

            movie.poster = s.image_for_movie(movie_id, ImageKind::Poster)?;
            movie.banner = s.image_for_movie(movie_id, ImageKind::Banner)?;

            Ok(Some(movie))
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn update_movie(
        &self,
        id: MovieId,
        title: Option<&str>,
        release_date: Option<Timestamp>,
        overview: Option<&str>,
    ) -> Result<()> {
        let title = title.map(str::to_owned);
        let overview = overview.map(str::to_owned);
        let mut s = self.inner.clone().exclusive().await?;

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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_release_date(
        &self,
        id: MovieId,
        release_date: Option<Timestamp>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_movie_release_date
                .execute((release_date.as_ref(), id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn delete_movie(&self, id: MovieId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_movie.execute((id,))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_tracked(&self, id: MovieId, tracked: bool) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_movie_tracked.execute((tracked, id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_auto_sync(&self, id: MovieId, auto_sync: bool) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_movie_auto_sync.execute((auto_sync, id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_remote_enabled(
        &self,
        remote_id: RemoteId,
        enabled: bool,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_movie_remote_enabled.execute((enabled, remote_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_remote_sync_kinds(
        &self,
        remote_id: RemoteId,
        sync_kinds: Option<api::SyncKindSet>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_movie_remote_sync_kinds
                .execute((sync_kinds, remote_id))?;
            Ok(())
        });

        result.await?
    }

    /// Set remote priority to match the given order (first = highest priority).
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn reorder_movie_remotes(&self, remote_ids: Vec<RemoteId>) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            for (idx, id) in remote_ids.iter().enumerate() {
                s.set_movie_remote_priority.execute((idx as i32, *id))?;
            }
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_language(
        &self,
        id: MovieId,
        language: Option<String>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let mut settings = current_movie_settings(&mut s, id)?;
            settings.language = language;
            s.upsert_movie_settings
                .execute((id, api::encode_movie_settings(&settings)))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_release_filters(
        &self,
        id: MovieId,
        release_filters: Option<Vec<api::ReleaseFilter>>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let mut settings = current_movie_settings(&mut s, id)?;
            settings.release_filters = release_filters;
            s.upsert_movie_settings
                .execute((id, api::encode_movie_settings(&settings)))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn clear_show_images(&self, show_id: ShowId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;
        spawn_blocking(move || {
            s.delete_show_images.execute((show_id,))?;
            Ok(())
        })
        .await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn clear_movie_images(&self, movie_id: MovieId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;
        spawn_blocking(move || {
            s.delete_movie_images.execute((movie_id,))?;
            Ok(())
        })
        .await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn clear_episode_images(&self, show_id: ShowId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;
        spawn_blocking(move || {
            s.delete_episode_images_for_show.execute((show_id,))?;
            Ok(())
        })
        .await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn upsert_episode_image(
        &self,
        id: ImageId,
        episode_id: EpisodeId,
        kind: ImageKind,
        image: &Image,
    ) -> Result<()> {
        let image = image.clone();
        let mut s = self.inner.clone().exclusive().await?;

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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_episode_image_selection(
        &self,
        episode_id: EpisodeId,
        kind: ImageKind,
        image_id: ImageId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_episode_image_selection
                .execute((episode_id, kind, image_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn upsert_show_image(
        &self,
        id: ImageId,
        show_id: ShowId,
        kind: ImageKind,
        rank: u32,
        image: &Image,
    ) -> Result<()> {
        let image = image.clone();
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_show_image.execute((
                id,
                show_id,
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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn upsert_movie_image(
        &self,
        id: ImageId,
        movie_id: MovieId,
        kind: ImageKind,
        rank: u32,
        image: &Image,
    ) -> Result<()> {
        let image = image.clone();
        let mut s = self.inner.clone().exclusive().await?;

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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_image_selection(
        &self,
        show_id: ShowId,
        kind: ImageKind,
        image_id: ImageId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_show_image_selection
                .execute((show_id, kind, image_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_image_selection(
        &self,
        movie_id: MovieId,
        kind: ImageKind,
        image_id: ImageId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_movie_image_selection
                .execute((movie_id, kind, image_id))?;
            Ok(())
        });

        result.await?
    }

    /// Selects the given image for its owning entity + kind, replacing any
    /// prior selection. Returns which entity owns the image.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn select_image(&self, id: ImageId) -> Result<api::ImageOwner> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let row = s
                .image_by_id
                .bind((id,))?
                .first()?
                .context("Expected image to exist")?;

            let kind = row.kind;

            let owner = match (row.show_id, row.movie_id, row.season_id) {
                (Some(show_id), _, _) => {
                    s.set_show_image_selection.execute((show_id, kind, id))?;
                    api::ImageOwner::Show(show_id)
                }
                (_, Some(movie_id), _) => {
                    s.set_movie_image_selection.execute((movie_id, kind, id))?;
                    api::ImageOwner::Movie(movie_id)
                }
                (_, _, Some(season_id)) => {
                    s.set_season_image_selection
                        .execute((season_id, kind, id))?;
                    api::ImageOwner::Season(season_id)
                }
                _ => anyhow::bail!("Image has no owner"),
            };

            Ok(owner)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn clear_selected_image(
        &self,
        owner: api::ImageOwner,
        kind: ImageKind,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            match owner {
                api::ImageOwner::Show(show_id) => {
                    s.delete_show_image_selection.execute((show_id, kind))?;
                }
                api::ImageOwner::Movie(movie_id) => {
                    s.delete_movie_image_selection.execute((movie_id, kind))?;
                }
                api::ImageOwner::Season(season_id) => {
                    s.delete_season_image_selection.execute((season_id, kind))?;
                }
            }

            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn season_images_by_id(
        &self,
        season_id: SeasonId,
    ) -> Result<Vec<api::MediaImage>> {
        let mut s = self.inner.clone().shared().await?;

        spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.list_season_images.bind((season_id,))?;
            while let Some(r) = stmt.next()? {
                out.push(image_from_row(r));
            }
            Ok(out)
        })
        .await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn show_id_for_season(&self, season_id: SeasonId) -> Result<Option<ShowId>> {
        let mut s = self.inner.clone().shared().await?;
        spawn_blocking(move || Ok(s.show_id_for_season.bind((season_id,))?.first()?)).await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn mark_watched(
        &self,
        id: WatchedId,
        kind: WatchedKind,
        mark_time: MarkTime,
        now: Timestamp,
    ) -> Result<api::Watched> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let (watched_id, timestamp) = match kind {
                WatchedKind::Episode { episode, .. } => {
                    let timestamp = s.episode_mark_time(episode, mark_time, now)?;

                    let key = s
                        .episode_natural_key
                        .bind((episode,))?
                        .first()?
                        .context("Expected episode to exist")?;

                    s.insert_watched_episode.execute((
                        id,
                        timestamp,
                        key.show_id,
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
                            .first()?
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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn insert_watched_episode(
        &self,
        id: WatchedId,
        timestamp: Timestamp,
        show_id: ShowId,
        season: api::SeasonNumber,
        episode: u32,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_watched_episode
                .execute((id, timestamp, show_id, season, episode))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn insert_watched_movie(
        &self,
        id: WatchedId,
        timestamp: Timestamp,
        movie_id: MovieId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_watched_movie.execute((id, timestamp, movie_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn move_watched_episode(
        &self,
        id: WatchedId,
        season: api::SeasonNumber,
        episode: u32,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.move_watched_episode.execute((season, episode, id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn orphaned_for_show(
        &self,
        show_id: ShowId,
    ) -> Result<Vec<api::OrphanedWatched>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_orphaned_for_show.bind((show_id,))?;

            while let Some(r) = stmt.next()? {
                out.push(api::OrphanedWatched {
                    id: r.id,
                    timestamp: r.timestamp,
                    show_id: r.show_id,
                    season: r.season,
                    episode: r.episode,
                });
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn remove_watched(&self, id: WatchedId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_watched_episode.execute((id,))?;
            s.delete_watched_movie.execute((id,))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn watched_for_episode(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_watched_by_episode.bind((episode_id,))?;

            while let Some(r) = stmt.next()? {
                out.push(watched_from_row(r)?);
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn watched_for_movie(&self, movie_id: MovieId) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_watched_by_movie.bind((movie_id,))?;

            while let Some(r) = stmt.next()? {
                out.push(watched_from_row(r)?);
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn add_pending_episode(
        &self,
        show_id: api::ShowId,
        episode_id: api::EpisodeId,
        ts: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.upsert_pending_episode
                .execute((PendingId::random(), ts, show_id, episode_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn add_pending_movie(
        &self,
        movie_id: api::MovieId,
        ts: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.upsert_pending_movie
                .execute((PendingId::random(), ts, movie_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn remove_pending_episode(&self, show_id: api::ShowId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_pending_episode.execute((show_id,))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn skip_pending_episode(
        &self,
        show_id: api::ShowId,
        episode_id: api::EpisodeId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let next_id = s
                .next_episode_after
                .bind((show_id, episode_id))?
                .first()?
                .map(|r| r.0);

            match next_id {
                Some(next) => {
                    let ts = Timestamp::now();
                    s.upsert_pending_episode
                        .execute((PendingId::random(), ts, show_id, next))?;
                }
                None => {
                    s.delete_pending_episode.execute((show_id,))?;
                }
            }

            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn remove_pending_movie(&self, movie_id: api::MovieId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_pending_movie.execute((movie_id,))?;
            Ok(())
        });

        result.await?
    }

    /// Fill the pending slot for a show, but ONLY if it currently has no pending episode.
    /// Called after sync upserts episodes, and after MarkWatched clears the old pending row.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn fill_pending_for_show(
        &self,
        show_id: api::ShowId,
        include_specials: bool,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let already_has = s
                .has_pending_episode_for_show
                .bind((show_id,))?
                .first()?
                .is_some();

            if already_has {
                // If the pending episode has a future air date that changed, update the timestamp.
                let maybe_update = {
                    let row = s.pending_episode_aired_for_show.bind((show_id,))?.first()?;
                    row.and_then(|r| {
                        let aired = r.aired?;
                        if aired > now && aired != r.timestamp {
                            Some(aired)
                        } else {
                            None
                        }
                    })
                };
                if let Some(aired) = maybe_update {
                    s.update_pending_episode_timestamp
                        .execute((aired, show_id))?;
                }
                return Ok(());
            }

            let Some(row) = s
                .next_pending_episode_for_show
                .bind((show_id, include_specials))?
                .first()?
            else {
                return Ok(());
            };

            let ts = row.aired.unwrap_or(now).max(now);

            s.upsert_pending_episode
                .execute((PendingId::random(), ts, show_id, row.id))?;

            Ok(())
        });

        result.await?
    }

    /// Fill the pending slot for a show, but ONLY if it currently has no pending episode.
    /// Called after sync upserts episodes, and after MarkWatched clears the old pending row.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn fill_pending_for_show_from(
        &self,
        show_id: api::ShowId,
        episode_id: api::EpisodeId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let already_has = s
                .has_pending_episode_for_show
                .bind((show_id,))?
                .first()?
                .is_some();

            if already_has {
                return Ok(());
            }

            let Some((next_id, aired)) =
                s.next_episode_after.bind((show_id, episode_id))?.first()?
            else {
                return Ok(());
            };

            let now = aired.unwrap_or(now).max(now);

            s.upsert_pending_episode
                .execute((PendingId::random(), now, show_id, next_id))?;

            Ok(())
        });

        result.await?
    }

    /// Like `fill_pending_for_show` but for bulk import: finds the first unwatched episode
    /// regardless of whether it has aired, and uses the actual aired timestamp rather than
    /// clamping to `now`. This preserves the episode's original air date as the pending
    /// timestamp so dashboard ordering reflects episode order rather than import time.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn fill_pending_for_show_import(&self, show_id: api::ShowId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let stmt = s.has_pending_episode_for_show.bind((show_id,))?;

            let already_has = stmt.first()?.is_some();

            if already_has {
                return Ok(());
            }

            let stmt = s.first_unwatched_episode_for_show.bind((show_id,))?;

            let Some(row) = stmt.first()? else {
                return Ok(());
            };

            let Some(ts) = row.aired else {
                return Ok(());
            };

            s.upsert_pending_episode
                .execute((PendingId::random(), ts, show_id, row.id))?;

            Ok(())
        });

        result.await?
    }

    /// Tracked movies that are not yet pending or watched, paired with their per-movie release
    /// filter override (raw JSON, `None` = use global default).
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_pending_candidates(&self) -> Result<Vec<(MovieId, Option<String>)>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.movie_pending_candidates.query()?;

            while let Some(r) = stmt.next()? {
                out.push((r.id, r.release_filters));
            }

            Ok(out)
        });

        result.await?
    }

    /// Whether the given movie has any watches.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn has_movie_watches(&self, id: MovieId) -> Result<bool> {
        let mut s = self.inner.clone().shared().await?;

        let result =
            spawn_blocking(move || Ok(s.has_watched_movie.bind((id,))?.first()?.is_some()));

        result.await?
    }

    /// The release dates recorded for a movie.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_releases(&self, id: MovieId) -> Result<Vec<api::MovieRelease>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_movie_releases.bind((id,))?;

            while let Some(r) = stmt.next()? {
                out.push(api::MovieRelease {
                    country: r.country,
                    release_type: r.release_type,
                    timestamp: r.timestamp,
                });
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_synced_at(&self, id: ShowId, at: Timestamp) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;
        let result = spawn_blocking(move || {
            s.set_show_synced_at.execute((at, id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_synced_at(&self, id: MovieId, at: Timestamp) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;
        let result = spawn_blocking(move || {
            s.set_movie_synced_at.execute((at, id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn upsert_movie_release(
        &self,
        movie_id: MovieId,
        country: &str,
        release_type: ReleaseType,
        timestamp: &Timestamp,
    ) -> Result<()> {
        let country = country.to_owned();
        let timestamp = *timestamp;
        let mut s = self.inner.clone().exclusive().await?;

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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn shows_needing_sync(&self, interval_hours: u32) -> Result<Vec<api::Show>> {
        let cutoff = cutoff_timestamp(interval_hours);
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.shows_needing_sync.bind((cutoff,))?;

            while let Some(row) = stmt.next()? {
                out.push(show_from_row(row));
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movies_needing_sync(&self, interval_hours: u32) -> Result<Vec<api::Movie>> {
        let cutoff = cutoff_timestamp(interval_hours);
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.movies_needing_sync.bind((cutoff,))?;

            while let Some(r) = stmt.next()? {
                out.push(movie_from_row(r));
            }

            Ok(out)
        });

        result.await?
    }

    /// Unified pending list replacing pending_episodes + pending_movies.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn pending(&self, now: Timestamp) -> Result<Vec<api::Pending>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            // Collect the base rows first so the iterating statement is released
            // before the per-row detail lookups below reuse the connection.
            let mut rows = Vec::new();
            let mut stmt = s.list_pending_before.bind((now,))?;

            while let Some(r) = stmt.next()? {
                rows.push(r);
            }

            stmt.reset()?;

            'outer: for r in rows {
                let pending = 'pending: {
                    if let Some(episode_id) = r.episode_id {
                        let detail = s.pending_episode_detail.bind((episode_id,))?.first()?;

                        let Some(d) = detail else {
                            continue 'outer;
                        };

                        let poster = s.image_for_show(d.show_id, ImageKind::Poster)?;
                        let banner = s.image_for_show(d.show_id, ImageKind::Banner)?;

                        break 'pending api::Pending {
                            kind: api::PendingKind::Episode {
                                show: d.show_id,
                                episode: episode_id,
                            },
                            info: api::PendingInfo::Episode {
                                show: d.show_title,
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
                        let detail = s.pending_movie_detail.bind((movie_id,))?.first()?;

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

    #[tracing::instrument(skip(self), ret(level = "trace"))]
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

        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            type DayShows = Vec<(ShowId, String, Vec<api::ScheduleEpisode>)>;
            let mut days_map = Vec::<(Date, DayShows, Vec<api::ScheduleMovie>)>::new();

            let mut stmt = s.list_schedule.bind((today, end))?;

            while let Some(r) = stmt.next()? {
                let Some(day) = r.aired else { continue };

                let ep = api::ScheduleEpisode {
                    season: r.season,
                    episode: r.number,
                    aired: day,
                };

                let day = day.date(tz.clone());

                if let Some(day_entry) = days_map.iter_mut().find(|(d, ..)| d == &day) {
                    if let Some(show_entry) =
                        day_entry.1.iter_mut().find(|(id, ..)| *id == r.show_id)
                    {
                        show_entry.2.push(ep);
                    } else {
                        day_entry.1.push((r.show_id, r.show_title, vec![ep]));
                    }
                } else {
                    days_map.push((day, vec![(r.show_id, r.show_title, vec![ep])], Vec::new()));
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_schedule_movies.bind((today, end))?;

            while let Some(r) = stmt.next()? {
                let Some(released) = r.released else { continue };

                let movie = api::ScheduleMovie {
                    movie_id: r.movie_id,
                    title: r.title,
                    released,
                };

                let day = released.date(tz.clone());

                if let Some(day_entry) = days_map.iter_mut().find(|(d, ..)| d == &day) {
                    day_entry.2.push(movie);
                } else {
                    days_map.push((day, Vec::new(), vec![movie]));
                }
            }

            // Movie-only days may be appended out of order; sort so the frontend can rely
            // on the last day being the furthest date when extending the calendar grid.
            days_map.sort_by(|(a, ..), (b, ..)| a.cmp(b));

            let out = days_map
                .into_iter()
                .map(|(date, show, movies)| api::ScheduledDay {
                    date,
                    shows: show
                        .into_iter()
                        .map(|(show_id, show_title, episodes)| api::ScheduledEntry {
                            show_id,
                            show_title,
                            episodes,
                        })
                        .collect(),
                    movies,
                })
                .collect();

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn load_config(&self) -> Result<Config> {
        let mut s = self.inner.clone().shared().await?;

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
            let include_specials = s
                .get_config("include_specials")?
                .map(|v| v == "true")
                .unwrap_or(false);

            let release_filters = s
                .get_config("release_filters")?
                .as_deref()
                .and_then(api::decode_release_filters)
                .unwrap_or_else(api::ReleaseFilter::default_filters);

            let air_date_filters = s
                .get_config("air_date_filters")?
                .as_deref()
                .and_then(api::decode_air_date_filters)
                .unwrap_or_default();

            let sync_kinds = s
                .get_config("sync_kinds")?
                .as_deref()
                .and_then(api::decode_sync_kinds)
                .unwrap_or_default();

            Ok(Config {
                theme,
                tvdb_api_key,
                tvdb_pin,
                tmdb_api_key,
                dashboard_page,
                auto_sync_enabled,
                auto_sync_interval_hours,
                timezone,
                language,
                include_specials,
                release_filters,
                air_date_filters,
                sync_kinds,
            })
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn save_config(&self, config: &Config) -> Result<()> {
        let config = config.clone();

        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_config("theme", config.theme.to_string().as_str())?;

            s.set_config("tvdb_api_key", &config.tvdb_api_key)?;

            if let Some(ref pin) = config.tvdb_pin {
                s.set_config("tvdb_pin", pin)?;
            } else {
                s.delete_config("tvdb_pin")?;
            }

            s.set_config("tmdb_api_key", &config.tmdb_api_key)?;

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
            s.set_config(
                "include_specials",
                if config.include_specials {
                    "true"
                } else {
                    "false"
                },
            )?;
            s.set_config(
                "release_filters",
                &api::encode_release_filters(&config.release_filters),
            )?;
            s.set_config(
                "air_date_filters",
                &api::encode_air_date_filters(&config.air_date_filters),
            )?;
            s.set_config("sync_kinds", &api::encode_sync_kinds(&config.sync_kinds))?;
            Ok(())
        });

        result.await?
    }

    /// The most-used per-show/per-movie custom language overrides, ordered
    /// most-used first, as recomputed by the periodic task.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn get_state_top_languages(&self) -> Result<Vec<String>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let raw = s.get_state_top_languages.query()?.next()?;
            Ok(raw
                .as_deref()
                .and_then(|v| serde_json::from_str::<Vec<String>>(v).ok())
                .unwrap_or_default())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_state_top_languages(&self, languages: Vec<String>) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let encoded = serde_json::to_string(&languages).unwrap_or_else(|_| "[]".to_string());
            s.set_state_top_languages.execute((encoded,))?;
            Ok(())
        });

        result.await?
    }

    /// Tally the effective custom language of every show and movie (the settings
    /// JSON blob's language when present, else the legacy column) and return the
    /// `n` most-used codes, ordered most-used first (ties broken by code).
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn compute_top_languages(&self, n: usize) -> Result<Vec<String>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut counts: HashMap<String, usize> = HashMap::new();

            let mut tally = |language: Option<String>| {
                if let Some(lang) = language.filter(|l| !l.trim().is_empty()) {
                    *counts.entry(lang).or_default() += 1;
                }
            };

            let mut stmt = s.list_show_languages.query()?;
            while let Some(row) = stmt.next()? {
                let language = match row
                    .settings_data
                    .as_deref()
                    .and_then(api::decode_show_settings)
                {
                    Some(settings) => settings.language,
                    None => row.language,
                };
                tally(language);
            }
            stmt.reset()?;

            let mut stmt = s.list_movie_languages.query()?;
            while let Some(row) = stmt.next()? {
                let language = match row
                    .settings_data
                    .as_deref()
                    .and_then(api::decode_movie_settings)
                {
                    Some(settings) => settings.language,
                    None => row.language,
                };
                tally(language);
            }
            stmt.reset()?;

            let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
            // Most-used first; break ties by code for a stable result.
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

            Ok(ranked.into_iter().take(n).map(|(lang, _)| lang).collect())
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

/// The current per-show settings: the stored JSON blob if present, otherwise
/// synthesized from the legacy columns (un-migrated row), else defaults. Used by
/// the setters to read-modify-write a single field.
fn current_show_settings(s: &mut InnerWrite, id: ShowId) -> Result<api::ShowSettings> {
    let Some(row) = s.show_by_id.bind((id,))?.first()? else {
        return Ok(api::ShowSettings::default());
    };

    Ok(
        match row
            .settings_data
            .as_deref()
            .and_then(api::decode_show_settings)
        {
            Some(settings) => settings,
            None => api::ShowSettings {
                language: row.language,
                include_specials: row.include_specials,
                air_date_filters: row
                    .air_date_filters
                    .as_deref()
                    .and_then(api::decode_air_date_filters),
            },
        },
    )
}

/// The current per-movie settings; see [`current_show_settings`].
fn current_movie_settings(s: &mut InnerWrite, id: MovieId) -> Result<api::MovieSettings> {
    let Some(row) = s.movie_by_id.bind((id,))?.first()? else {
        return Ok(api::MovieSettings::default());
    };

    Ok(
        match row
            .settings_data
            .as_deref()
            .and_then(api::decode_movie_settings)
        {
            Some(settings) => settings,
            None => api::MovieSettings {
                language: row.language,
                release_filters: row
                    .release_filters
                    .as_deref()
                    .and_then(api::decode_release_filters),
            },
        },
    )
}

fn show_from_row(r: ShowRow) -> api::Show {
    // The settings blob, once present, is authoritative; otherwise fall back to
    // the legacy per-show columns (un-migrated rows).
    let settings = r
        .settings_data
        .as_deref()
        .and_then(api::decode_show_settings);

    let (language, include_specials, air_date_filters) = match settings {
        Some(s) => (s.language, s.include_specials, s.air_date_filters),
        None => (
            r.language,
            r.include_specials,
            r.air_date_filters
                .as_deref()
                .and_then(api::decode_air_date_filters),
        ),
    };

    api::Show {
        id: r.id,
        title: r.title,
        first_air_date: r.first_air,
        overview: r.overview,
        tracked: r.tracked,
        auto_sync: r.auto_sync,
        remotes: Vec::new(),
        images: Vec::new(),
        poster: None,
        banner: None,
        backdrop: None,
        last_synced_at: r.last_synced_at,
        language,
        include_specials,
        air_date_filters,
    }
}

/// Default merge priority for a freshly-added remote (lower wins). TVmaze ranks
/// highest so its air dates win by default; matches the migration backfill.
fn default_remote_priority(source: RemoteSource) -> i32 {
    match source {
        RemoteSource::Tvmaze => 0,
        RemoteSource::Tmdb => 2,
        RemoteSource::Tvdb => 3,
        RemoteSource::Imdb => 4,
        RemoteSource::Unknown => 9,
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

fn show_image_from_row(r: ShowImageRow) -> api::MediaImage {
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

fn apply_image_selection(target: &mut api::Show, r: ImageSelectionRow) {
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
        show_id: r.show_id,
        season: r.season,
        air_date: r.air_date,
        name: r.name,
        overview: r.overview,
        poster: match (r.poster_source, r.poster_path) {
            (Some(source), Some(path)) => Some(api::Image::new(source, &path)),
            _ => None,
        },
        watched_count: r.watched_count,
        total_count: r.total_count,
    }
}

fn episode_from_row(r: EpisodeRow) -> api::Episode {
    api::Episode {
        id: r.id,
        show_id: r.show_id,
        season: r.season,
        episode: r.number,
        absolute_number: r.absolute_number,
        name: r.name,
        overview: r.overview,
        aired: r.aired,
        remote_id: None,
        pending: r.pending,
        watched_count: r.watched_count,
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
    // The settings blob, once present, is authoritative; otherwise fall back to
    // the legacy per-movie columns (un-migrated rows).
    let settings = r
        .settings_data
        .as_deref()
        .and_then(api::decode_movie_settings);

    let (language, release_filters) = match settings {
        Some(s) => (s.language, s.release_filters),
        None => (
            r.language,
            r.release_filters
                .as_deref()
                .and_then(api::decode_release_filters),
        ),
    };

    api::Movie {
        id: r.id,
        title: r.title,
        release_date: r.release_date,
        overview: r.overview,
        remotes: Vec::new(),
        tracked: r.tracked,
        auto_sync: r.auto_sync,
        pending: false,
        images: Vec::new(),
        poster: None,
        banner: None,
        backdrop: None,
        last_synced_at: r.last_synced_at,
        releases: Vec::new(),
        language,
        release_filters,
    }
}

fn media_item_from_row(r: MediaItemRow, kind: api::MediaKind) -> api::MediaItem {
    api::MediaItem {
        id: r.id.cast_unsigned(),
        kind,
        title: r.title,
        date: r.date,
        overview: r.overview,
        poster: None,
        banner: None,
        backdrop: None,
        tracked: r.tracked,
        last_watched_at: None,
        remotes: Vec::new(),
    }
}

fn watched_from_row(r: WatchedRow) -> Result<api::Watched> {
    let kind = match (r.show_id, r.episode_id, r.movie_id) {
        (Some(show), Some(episode), None) => WatchedKind::Episode { show, episode },
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

    let mut ids: Vec<_> = Migrations::iter().collect();
    ids.sort();

    for file in ids {
        let id = file.as_ref();

        let result: Result<()> = (|| {
            select.reset()?;
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
            insert.reset()?;
            insert
                .execute((id, now.as_str()))
                .with_context(|| anyhow!("Updating migrations table {id}"))?;
            tracing::info!(id, "Migration applied");
            Ok(())
        })();

        result.with_context(|| anyhow!("Migration {id}"))?;
    }

    Ok(())
}

fn ensure_mode(c: &mut sqll::Connection, mode: OpenMode) -> Result<(), sqll::Error> {
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

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn migrations_apply_on_fresh_db() {
        let dir = std::env::temp_dir().join(format!("ontv-mig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);

        let c = OpenOptions::new()
            .extended_result_codes()
            .read_write()
            .create()
            .no_mutex()
            .open(path.as_os_str())
            .unwrap();

        do_migrations(&c).expect("migrations should apply");

        let _ = std::fs::remove_file(&path);
    }
}
