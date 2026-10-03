#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

use core::ops::{Deref, DerefMut};
use core::str;

use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use std::collections::{HashMap, HashSet};

use api::{
    Config, Country, Credit, CreditId, CreditKind, Date, EpisodeId, Image, ImageId, ImageKey,
    ImageKind, ImageSource, IncludeSpecials, MarkTime, MovieId, PendingId, PersonId, PreferenceKey,
    PreferenceValue, ReleaseType, Remote, RemoteId, RemoteSource, RemoteValue, SeasonId,
    SeasonNumber, ShowId, Timestamp, UserId, WatchedId, WatchedKind,
};
use rust_embed::RustEmbed;
use sqll::{OpenOptions, Pool, PoolBuilder, Row, Statements, TypedStatement};
use tokio::task::spawn_blocking;

#[cfg(test)]
mod tests;

pub(crate) mod config;
use self::config::Pref;
pub(crate) mod users;

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
    first_air: Option<Timestamp>,
    tracked: bool,
    auto_sync: bool,
    last_synced_at: Option<Timestamp>,
    language: Pref<api::Locale>,
    default_language: api::Locale,
    include_specials: Pref<api::IncludeSpecials>,
    air_date_filters: Option<String>,
}

/// A per-show or per-movie language some user picked, to tally the most-used
/// ones.
#[derive(Row)]
struct LanguageRow {
    language: Pref<api::Locale>,
}

#[derive(Row)]
struct ImageRow {
    id: ImageId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    score: Option<f64>,
}

/// Kind + (source, path) of a user-chosen selection, used to re-attach it to the
/// freshly inserted image row after a sync clears and rebuilds images.
#[derive(Row)]
struct UserSelectedRow {
    kind: ImageKind,
    source: ImageSource,
    path: String,
}

#[derive(Row)]
struct KindRankRow {
    kind: ImageKind,
    rank: u32,
}

#[derive(Row)]
struct ShowImageRow {
    id: ImageId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    score: Option<f64>,
    show_id: ShowId,
}

#[derive(Row)]
struct MovieImageRow {
    id: ImageId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    score: Option<f64>,
    movie_id: MovieId,
}

/// A credit row plus its person's best-ranked profile image. Person names and
/// character names are folded in separately from the `*_strings` tables.
#[derive(Row)]
struct CreditRow {
    id: CreditId,
    person_id: PersonId,
    credit_type: CreditKind,
    department: Option<String>,
    job: Option<String>,
    sort_order: Option<u32>,
    episode_count: Option<u32>,
    profile_source: Option<ImageSource>,
    profile_path: Option<String>,
}

/// A person needing a sync, with a best-effort display name for the task label.
#[derive(Row)]
struct PersonSyncRow {
    id: PersonId,
    name: Option<String>,
}

/// A person's own row; identity/remotes (incl. IMDb) live in `person_remotes`.
#[derive(Row)]
struct PersonRow {
    department: Option<String>,
    default_language: api::Locale,
    last_synced_at: Option<Timestamp>,
}

/// A slim person row for the people list view.
#[derive(Row)]
struct PersonListRow {
    id: PersonId,
    department: Option<String>,
    default_language: api::Locale,
}

/// A person's best-ranked profile image, carrying the person id for bulk grouping.
#[derive(Row)]
struct PersonProfileRow {
    person_id: PersonId,
    source: ImageSource,
    path: String,
    width: u32,
    height: u32,
}

/// A single person's best-ranked profile image (single-person lookup).
#[derive(Row)]
struct ProfileRow {
    source: ImageSource,
    path: String,
    width: u32,
    height: u32,
}

/// A person's credit on a show or movie, with the owner's date and best poster.
/// Character names are folded in separately from `*_credit_strings`.
#[derive(Row)]
struct PersonCreditRow {
    id: CreditId,
    /// The owning show/movie id, stored bit-reinterpreted as a signed integer
    /// (like every id column); recovered with `as u64` at the call site.
    owner_id: i64,
    /// The owner's original language, used to resolve its title when no configured
    /// display language matches (mirrors show/movie title resolution).
    owner_language: api::Locale,
    credit_type: CreditKind,
    department: Option<String>,
    job: Option<String>,
    sort_order: Option<u32>,
    episode_count: Option<u32>,
    date: Option<Timestamp>,
    poster_source: Option<ImageSource>,
    poster_path: Option<String>,
}

#[derive(Row)]
struct ImageSelectionRow {
    kind: ImageKind,
    source: ImageSource,
    path: String,
    width: u32,
    height: u32,
    user_selected: bool,
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
    user_selected: bool,
}

#[derive(Row)]
struct AllMovieImageSelectionRow {
    movie_id: MovieId,
    kind: ImageKind,
    source: ImageSource,
    path: String,
    width: u32,
    height: u32,
    user_selected: bool,
}

#[derive(Row)]
struct SeasonRow {
    id: SeasonId,
    show_id: ShowId,
    season: SeasonNumber,
    air_date: Option<Timestamp>,
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
    aired: Option<Timestamp>,
    pending: Option<Timestamp>,
    watched_count: u32,
}

/// An episode due a per-episode sync, with the identity the task kind needs.
#[derive(Row)]
struct EpisodeAirSyncRow {
    id: EpisodeId,
    show_id: ShowId,
    season: SeasonNumber,
    episode: u32,
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
    release_date: Option<Timestamp>,
    tracked: bool,
    auto_sync: bool,
    last_synced_at: Option<Timestamp>,
    language: Pref<api::Locale>,
    default_language: api::Locale,
    release_filters: Option<String>,
}

#[derive(Row)]
struct MovieReleaseRow {
    source: RemoteSource,
    country: Country,
    release_type: ReleaseType,
    timestamp: Timestamp,
}

/// One stored air date for an episode (joined back onto its episode in Rust).
#[derive(Row)]
struct EpisodeReleaseRow {
    episode_id: EpisodeId,
    source: RemoteSource,
    country: Country,
    network: String,
    timestamp: Timestamp,
}

#[derive(Row)]
struct MediaItemRow {
    // SQLite stores ids as signed integers; reinterpret to u64 (matches `define_id`).
    id: i64,
    date: Option<Timestamp>,
    tracked: bool,
    language: Pref<api::Locale>,
    default_language: api::Locale,
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
struct WatchOrderNextRow {
    show_id: ShowId,
    special: bool,
    episode_id: EpisodeId,
}

#[derive(Row)]
struct UnwatchedShowRow {
    show_id: ShowId,
    unwatched: i64,
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
    timestamp: Timestamp,
}

#[derive(Row)]
struct PendingEpisodeDetailRow {
    show_id: api::ShowId,
    language: Pref<api::Locale>,
    default_language: api::Locale,
    season: SeasonNumber,
    number: u32,
    aired: Option<Timestamp>,
}

#[derive(Row)]
struct PendingMovieDetailRow {
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
struct EpisodeMatchRow {
    season: SeasonNumber,
    episode: u32,
}

#[derive(Row)]
struct PendingEpisodeAiredRow {
    aired: Option<Timestamp>,
    timestamp: Timestamp,
}

#[derive(Row)]
struct MoviePendingCandidateRow {
    user_id: UserId,
    id: api::MovieId,
    release_filters: Option<String>,
}

#[derive(Row)]
struct ScheduleRow {
    show_id: ShowId,
    season: SeasonNumber,
    number: u32,
    aired: Option<Timestamp>,
}

#[derive(Row)]
struct ScheduleMovieRow {
    movie_id: MovieId,
    released: Option<Timestamp>,
}

/// Best-effort parse of a remote's stored cache JSON into [`api::RemoteCache`].
/// A `NULL`, empty, or unparsable value yields `None` — the next sync overwrites it.
fn parse_remote_cache(raw: Option<String>) -> Option<api::RemoteCache> {
    serde_json::from_str(raw.as_deref()?).ok()
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
    cache: Option<String>,
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
    cache: Option<String>,
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
    cache: Option<String>,
}

/// A remote flattened for backup export: its stable identifier, the owning
/// show/movie id, and its full structure.
#[derive(Debug)]
pub(crate) struct ExportRemote<Owner> {
    pub id: RemoteId,
    pub owner: Owner,
    pub source: RemoteSource,
    pub value: RemoteValue,
    pub slug: Option<String>,
    pub enabled: bool,
    pub priority: i32,
    pub sync_kinds: Option<api::SyncKindSet>,
}

/// Per-user tracking and preferences for backup export, each row naming its
/// user by login. Preference keys and values are left as stored.
#[derive(Debug, Default)]
pub(crate) struct ExportUserData {
    pub tracked_shows: Vec<(String, ShowId)>,
    pub tracked_movies: Vec<(String, MovieId)>,
    pub user_config: Vec<(String, String, String)>,
    pub show_config: Vec<(String, ShowId, String, String)>,
    pub movie_config: Vec<(String, MovieId, String, String)>,
}

/// Everything a backup holds, read from one snapshot of the database.
#[derive(Debug)]
pub(crate) struct ExportSnapshot {
    pub users: Vec<users::UserRecord>,
    pub show_remotes: Vec<ExportRemote<ShowId>>,
    pub movie_remotes: Vec<ExportRemote<MovieId>>,
    pub user_data: ExportUserData,
    pub watched_episodes: Vec<(WatchedId, String, Timestamp, ShowId, SeasonNumber, u32)>,
    pub watched_movies: Vec<(WatchedId, String, Timestamp, MovieId)>,
}

fn all_rows<O>(stmt: &mut TypedStatement<(), O>) -> Result<Vec<O>>
where
    O: for<'stmt> Row<'stmt>,
{
    let mut out = Vec::new();
    let mut rows = stmt.query()?;

    while let Some(row) = rows.next()? {
        out.push(row);
    }

    rows.reset()?;
    Ok(out)
}

/// A watched-episode row for backup export (`list_all_watched_episodes`). The
/// `show_id` is nullable in the table; orphaned rows are skipped on export.
#[derive(Row)]
struct AllWatchedEpisodeRow {
    id: WatchedId,
    login: String,
    timestamp: Timestamp,
    show_id: Option<ShowId>,
    season: SeasonNumber,
    episode: u32,
}

/// A watched-movie row for backup export (`list_all_watched_movies`). The
/// `movie_id` is nullable in the table; orphaned rows are skipped on export.
#[derive(Row)]
struct AllWatchedMovieRow {
    id: WatchedId,
    login: String,
    timestamp: Timestamp,
    movie_id: Option<MovieId>,
}

/// A single localized string grouped onto its owning entity in Rust, used by the
/// bulk `list_all_*_strings` / `list_show_*_strings` queries to build each
/// entity's [`api::Translations`].
#[derive(Row)]
struct AllShowStringRow {
    show_id: ShowId,
    language: api::Locale,
    kind: api::StringKind,
    text: String,
}

#[derive(Row)]
struct AllMovieStringRow {
    movie_id: MovieId,
    language: api::Locale,
    kind: api::StringKind,
    text: String,
}

#[derive(Row)]
struct AllSeasonStringRow {
    season_id: SeasonId,
    language: api::Locale,
    kind: api::StringKind,
    text: String,
}

#[derive(Row)]
struct AllEpisodeStringRow {
    episode_id: EpisodeId,
    language: api::Locale,
    kind: api::StringKind,
    text: String,
}

/// The two locales needed to resolve an entity's strings: the owning show/movie's
/// configured display `language` and its `default_language` (original).
#[derive(Row)]
struct EntityLocaleRow {
    language: Pref<api::Locale>,
    default_language: api::Locale,
}

#[derive(Statements)]
#[sql(read_only)]
struct InnerTranslations {
    #[sql = "SELECT (SELECT c.value FROM user_show_config c WHERE c.user_id = ?1 AND c.show_id = shows.id AND c.key = 'language') AS language, default_language FROM shows WHERE id = ?2"]
    show_locales: TypedStatement<(Option<UserId>, ShowId), EntityLocaleRow>,
    #[sql = "SELECT (SELECT c.value FROM user_movie_config c WHERE c.user_id = ?1 AND c.movie_id = movies.id AND c.key = 'language') AS language, default_language FROM movies WHERE id = ?2"]
    movie_locales: TypedStatement<(Option<UserId>, MovieId), EntityLocaleRow>,
    #[sql = "SELECT language, kind, text FROM episode_strings WHERE episode_id = ? ORDER BY kind, language"]
    list_episode_strings: TypedStatement<(EpisodeId,), (api::Locale, api::StringKind, String)>,
    #[sql = "SELECT language, kind, text FROM show_strings WHERE show_id = ? ORDER BY kind, language"]
    list_show_strings: TypedStatement<(ShowId,), (api::Locale, api::StringKind, String)>,
    #[sql = "SELECT language, kind, text FROM movie_strings WHERE movie_id = ? ORDER BY kind, language"]
    list_movie_strings: TypedStatement<(MovieId,), (api::Locale, api::StringKind, String)>,
}

impl InnerTranslations {
    /// Load the full [`api::Translations`] for one show, inserting each row
    /// straight off the advancing statement.
    fn show(
        &mut self,
        user: impl Into<Option<UserId>>,
        id: ShowId,
        config: api::Locale,
    ) -> Result<api::Translations> {
        let (language, default) = self
            .show_locales
            .bind((user.into(), id))?
            .first()?
            .map(|r| (r.language.0, r.default_language))
            .unwrap_or_default();

        let mut translations = api::Translations::new(language.or(config).or(default));
        let mut stmt = self.list_show_strings.bind((id,))?;

        while let Some((locale, kind, text)) = stmt.next()? {
            translations.insert(kind, locale, &text);
        }

        stmt.reset()?;
        Ok(translations)
    }

    /// Load the full [`api::Translations`] for one movie.
    fn movie(
        &mut self,
        user: impl Into<Option<UserId>>,
        id: MovieId,
        config: api::Locale,
    ) -> Result<api::Translations> {
        let (language, default) = self
            .movie_locales
            .bind((user.into(), id))?
            .first()?
            .map(|r| (r.language.0, r.default_language))
            .unwrap_or_default();

        let mut translations = api::Translations::new(language.or(config).or(default));

        let mut stmt = self.list_movie_strings.bind((id,))?;

        while let Some((locale, kind, text)) = stmt.next()? {
            translations.insert(kind, locale, &text);
        }

        stmt.reset()?;
        Ok(translations)
    }

    /// Load the full [`api::Translations`] for one episode, resolved against its
    /// owning show's locales (passed in to avoid re-querying per episode).
    fn episode(
        &mut self,
        id: EpisodeId,
        language: api::Locale,
        default: api::Locale,
        config: api::Locale,
    ) -> Result<api::Translations> {
        let mut translations = api::Translations::new(language.or(config).or(default));
        let mut stmt = self.list_episode_strings.bind((id,))?;

        while let Some((locale, kind, text)) = stmt.next()? {
            translations.insert(kind, locale, &text);
        }

        stmt.reset()?;
        Ok(translations)
    }
}

#[derive(Statements)]
#[sql(read_only)]
struct InnerImage {
    #[sql = "SELECT i.source, i.path"]
    #[sql = "FROM show_images si JOIN show_image_candidates i ON i.id = si.image_id"]
    #[sql = "WHERE si.show_id = ? AND si.kind = ?"]
    image_for_show: TypedStatement<(ShowId, ImageKind), PendingImageRow>,
    #[sql = "SELECT i.source, i.path"]
    #[sql = "FROM movie_images mi JOIN movie_image_candidates i ON i.id = mi.image_id"]
    #[sql = "WHERE mi.movie_id = ? AND mi.kind = ?"]
    image_for_movie: TypedStatement<(MovieId, ImageKind), PendingImageRow>,
    #[sql = "SELECT i.source, i.path"]
    #[sql = "FROM season_images si"]
    #[sql = "JOIN seasons se ON se.id = si.season_id"]
    #[sql = "JOIN season_image_candidates i ON i.id = si.image_id"]
    #[sql = "WHERE se.show_id = ? AND se.season = ? AND si.kind = ?"]
    image_for_season: TypedStatement<(ShowId, SeasonNumber, ImageKind), PendingImageRow>,
}

impl InnerImage {
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

    fn image_for_season(
        &mut self,
        show_id: ShowId,
        season: SeasonNumber,
        kind: ImageKind,
    ) -> Result<Option<api::Image>> {
        let row = self
            .image_for_season
            .bind((show_id, season, kind))?
            .first()?;
        Ok(row.map(|p| api::Image::new(p.source, &p.path)))
    }
}

#[derive(Statements)]
#[sql(read_only)]
struct InnerEpisodes {
    #[sql = "SELECT aired FROM episodes WHERE id = ?"]
    episode_aired_by_id: TypedStatement<(EpisodeId,), Option<Timestamp>>,
}

impl InnerEpisodes {
    fn episode_mark_time(
        &mut self,
        episode: EpisodeId,
        mark_time: MarkTime,
        now: Timestamp,
    ) -> Result<Timestamp> {
        match mark_time {
            MarkTime::Now => Ok(now),
            MarkTime::At(ts) => Ok(ts),
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
}

#[derive(Statements)]
#[sql(read_only)]
pub(crate) struct InnerRead {
    #[sql(statements)]
    image: InnerImage,
    #[sql(statements)]
    translations: InnerTranslations,
    #[sql(statements)]
    episodes: InnerEpisodes,
    #[sql(statements)]
    users: users::Read,

    // shows
    // Shows and movies carry whether the viewer (?1, NULL for nobody) tracks them.
    #[sql = "SELECT shows.id, first_air, EXISTS (SELECT 1 FROM user_tracked_shows t WHERE t.show_id = shows.id AND t.user_id = ?1) AS tracked, auto_sync, last_synced_at, (SELECT c.value FROM user_show_config c WHERE c.user_id = ?1 AND c.show_id = shows.id AND c.key = 'language') AS language, default_language, (SELECT c.value FROM user_show_config c WHERE c.user_id = ?1 AND c.show_id = shows.id AND c.key = 'include-specials') AS include_specials, air_date_filters"]
    #[sql = "FROM shows ORDER BY shows.id"]
    list_shows: TypedStatement<(Option<UserId>,), ShowRow>,
    #[sql = "SELECT shows.id, first_air, EXISTS (SELECT 1 FROM user_tracked_shows t WHERE t.show_id = shows.id AND t.user_id = ?1) AS tracked, auto_sync, last_synced_at, (SELECT c.value FROM user_show_config c WHERE c.user_id = ?1 AND c.show_id = shows.id AND c.key = 'language') AS language, default_language, (SELECT c.value FROM user_show_config c WHERE c.user_id = ?1 AND c.show_id = shows.id AND c.key = 'include-specials') AS include_specials, air_date_filters"]
    #[sql = "FROM shows WHERE shows.id = ?2"]
    show_by_id: TypedStatement<(Option<UserId>, ShowId), ShowRow>,
    #[sql = "SELECT s.id, s.first_air, EXISTS (SELECT 1 FROM user_tracked_shows t WHERE t.show_id = s.id AND t.user_id = ?1) AS tracked, s.auto_sync, s.last_synced_at, (SELECT c.value FROM user_show_config c WHERE c.user_id = ?1 AND c.show_id = s.id AND c.key = 'language') AS language, s.default_language, (SELECT c.value FROM user_show_config c WHERE c.user_id = ?1 AND c.show_id = s.id AND c.key = 'include-specials') AS include_specials, s.air_date_filters"]
    #[sql = "FROM shows s"]
    #[sql = "JOIN show_remotes r ON r.show_id = s.id"]
    #[sql = "WHERE r.source = ?2 AND r.value = ?3"]
    shows_by_remote: TypedStatement<(Option<UserId>, RemoteSource, RemoteValue), ShowRow>,
    #[sql = "SELECT user_id FROM user_tracked_shows WHERE show_id = ?"]
    show_trackers: TypedStatement<(ShowId,), UserId>,
    #[sql = "SELECT user_id FROM user_tracked_movies WHERE movie_id = ?"]
    movie_trackers: TypedStatement<(MovieId,), UserId>,

    // remotes (one table per owner; source is a numeric enum, value is dynamic)
    #[sql = "SELECT id, slug, source, value, enabled, priority, sync_kinds, cache FROM show_remotes WHERE show_id = ? ORDER BY priority, id"]
    list_show_remotes: TypedStatement<(ShowId,), RemoteRow>,
    #[sql = "SELECT show_id, id, slug, source, value, enabled, priority, sync_kinds, cache FROM show_remotes ORDER BY show_id, priority, id"]
    list_all_show_remotes: TypedStatement<(), AllShowRemoteRow>,
    #[sql = "SELECT show_id, language, kind, text FROM show_strings ORDER BY show_id"]
    list_all_show_strings: TypedStatement<(), AllShowStringRow>,
    #[sql = "SELECT show_id FROM show_remotes WHERE source = ? AND value = ? LIMIT 1"]
    show_id_by_remote: TypedStatement<(RemoteSource, RemoteValue), ShowId>,

    // images (shows and movies share one table)
    #[sql = "SELECT id, kind, source, path, score FROM show_image_candidates"]
    #[sql = "WHERE show_id = ? ORDER BY kind, rank, id"]
    list_show_images: TypedStatement<(ShowId,), ImageRow>,
    #[sql = "SELECT id, kind, source, path, score, show_id FROM show_image_candidates"]
    #[sql = "ORDER BY show_id, kind, rank, id"]
    list_all_show_images: TypedStatement<(), ShowImageRow>,
    #[sql = "SELECT ei.episode_id, i.source, i.path, i.width, i.height"]
    #[sql = "FROM episode_images ei JOIN episode_image_candidates i ON i.id = ei.image_id"]
    #[sql = "WHERE ei.kind = ? AND ei.episode_id IN (SELECT id FROM episodes WHERE show_id = ? AND season = ?)"]
    list_season_episode_screenshots:
        TypedStatement<(ImageKind, ShowId, SeasonNumber), EpisodeScreenshotRow>,
    #[sql = "SELECT ei.episode_id, i.source, i.path, i.width, i.height"]
    #[sql = "FROM episode_images ei JOIN episode_image_candidates i ON i.id = ei.image_id"]
    #[sql = "WHERE ei.kind = ? AND ei.episode_id = ?"]
    episode_screenshot: TypedStatement<(ImageKind, EpisodeId), EpisodeScreenshotRow>,
    #[sql = "SELECT id, kind, source, path, score FROM movie_image_candidates"]
    #[sql = "WHERE movie_id = ? ORDER BY kind, rank, id"]
    list_movie_images: TypedStatement<(MovieId,), ImageRow>,
    #[sql = "SELECT id, kind, source, path, score, movie_id FROM movie_image_candidates"]
    #[sql = "ORDER BY movie_id, kind, rank, id"]
    list_all_movie_images: TypedStatement<(), MovieImageRow>,
    // Resolve the owner of a selected image id by probing each candidate table;
    // ids are globally unique, so at most one table matches.
    #[sql = "SELECT show_id, kind FROM show_image_candidates WHERE id = ?"]
    show_image_owner_by_id: TypedStatement<(ImageId,), (ShowId, ImageKind)>,
    #[sql = "SELECT movie_id, kind FROM movie_image_candidates WHERE id = ?"]
    movie_image_owner_by_id: TypedStatement<(ImageId,), (MovieId, ImageKind)>,
    #[sql = "SELECT season_id, kind FROM season_image_candidates WHERE id = ?"]
    season_image_owner_by_id: TypedStatement<(ImageId,), (SeasonId, ImageKind)>,
    #[sql = "SELECT id, kind, source, path, score FROM season_image_candidates"]
    #[sql = "WHERE season_id = ? ORDER BY kind, rank, id"]
    list_season_images: TypedStatement<(SeasonId,), ImageRow>,
    #[sql = "SELECT show_id FROM seasons WHERE id = ?"]
    show_id_for_season: TypedStatement<(SeasonId,), ShowId>,

    // credits (cast & crew; person + character names folded in from *_strings).
    // The person's best-ranked profile is pulled via correlated subqueries.
    #[sql = "SELECT c.id, c.person_id, c.credit_type, c.department, c.job, c.sort_order, c.episode_count,"]
    #[sql = "  (SELECT source FROM person_image_candidates WHERE person_id = c.person_id AND kind = 5 ORDER BY rank, id LIMIT 1) AS profile_source,"]
    #[sql = "  (SELECT path FROM person_image_candidates WHERE person_id = c.person_id AND kind = 5 ORDER BY rank, id LIMIT 1) AS profile_path"]
    #[sql = "FROM show_credits c"]
    #[sql = "WHERE c.show_id = ? ORDER BY c.credit_type, c.episode_count DESC, c.sort_order, c.id"]
    list_show_credits: TypedStatement<(ShowId,), CreditRow>,
    #[sql = "SELECT cs.credit_id, cs.language, cs.text FROM show_credit_strings cs"]
    #[sql = "JOIN show_credits c ON c.id = cs.credit_id"]
    #[sql = "WHERE c.show_id = ? ORDER BY cs.credit_id"]
    list_show_credit_strings: TypedStatement<(ShowId,), (CreditId, api::Locale, String)>,
    // Person names (kind=Title) for a show's credits, keyed by credit id.
    #[sql = "SELECT c.id, ps.language, ps.text FROM show_credits c"]
    #[sql = "JOIN person_strings ps ON ps.person_id = c.person_id AND ps.kind = 1"]
    #[sql = "WHERE c.show_id = ? ORDER BY c.id"]
    list_show_credit_names: TypedStatement<(ShowId,), (CreditId, api::Locale, String)>,
    #[sql = "SELECT c.id, c.person_id, c.credit_type, c.department, c.job, c.sort_order, c.episode_count,"]
    #[sql = "  (SELECT source FROM person_image_candidates WHERE person_id = c.person_id AND kind = 5 ORDER BY rank, id LIMIT 1) AS profile_source,"]
    #[sql = "  (SELECT path FROM person_image_candidates WHERE person_id = c.person_id AND kind = 5 ORDER BY rank, id LIMIT 1) AS profile_path"]
    #[sql = "FROM movie_credits c"]
    #[sql = "WHERE c.movie_id = ? ORDER BY c.credit_type, c.episode_count DESC, c.sort_order, c.id"]
    list_movie_credits: TypedStatement<(MovieId,), CreditRow>,
    #[sql = "SELECT cs.credit_id, cs.language, cs.text FROM movie_credit_strings cs"]
    #[sql = "JOIN movie_credits c ON c.id = cs.credit_id"]
    #[sql = "WHERE c.movie_id = ? ORDER BY cs.credit_id"]
    list_movie_credit_strings: TypedStatement<(MovieId,), (CreditId, api::Locale, String)>,
    #[sql = "SELECT c.id, ps.language, ps.text FROM movie_credits c"]
    #[sql = "JOIN person_strings ps ON ps.person_id = c.person_id AND ps.kind = 1"]
    #[sql = "WHERE c.movie_id = ? ORDER BY c.id"]
    list_movie_credit_names: TypedStatement<(MovieId,), (CreditId, api::Locale, String)>,
    // person reads
    #[sql = "SELECT department, default_language, last_synced_at FROM people WHERE id = ?"]
    person_row: TypedStatement<(PersonId,), PersonRow>,
    #[sql = "SELECT language, kind, text FROM person_strings WHERE person_id = ?"]
    list_person_strings: TypedStatement<(PersonId,), (api::Locale, api::StringKind, String)>,
    #[sql = "SELECT source, path, width, height FROM person_image_candidates"]
    #[sql = "WHERE person_id = ? AND kind = 5 ORDER BY rank, id LIMIT 1"]
    person_profile: TypedStatement<(PersonId,), ProfileRow>,
    #[sql = "SELECT id, slug, source, value, enabled, priority, sync_kinds, cache FROM person_remotes WHERE person_id = ? ORDER BY priority, id"]
    list_person_remotes: TypedStatement<(PersonId,), RemoteRow>,
    // people list (bulk, grouped in Rust like shows())
    #[sql = "SELECT id, department, default_language FROM people"]
    list_people: TypedStatement<(), PersonListRow>,
    #[sql = "SELECT person_id, language, text FROM person_strings WHERE kind = 1 ORDER BY person_id"]
    list_all_person_names: TypedStatement<(), (PersonId, api::Locale, String)>,
    #[sql = "SELECT person_id, source, path, width, height FROM person_image_candidates"]
    #[sql = "WHERE kind = 5 ORDER BY person_id, rank, id"]
    list_all_person_profiles: TypedStatement<(), PersonProfileRow>,
    #[sql = "SELECT person_id, COUNT(*) AS n FROM"]
    #[sql = "  (SELECT person_id FROM show_credits UNION ALL SELECT person_id FROM movie_credits)"]
    #[sql = "GROUP BY person_id"]
    list_person_credit_counts: TypedStatement<(), (PersonId, u32)>,
    // a person's filmography (shows + movies they are credited on)
    #[sql = "SELECT c.id, c.show_id AS owner_id, s.default_language AS owner_language, c.credit_type, c.department, c.job, c.sort_order, c.episode_count, s.first_air AS date,"]
    #[sql = "  (SELECT i.source FROM show_images si JOIN show_image_candidates i ON i.id = si.image_id WHERE si.show_id = c.show_id AND si.kind = 1) AS poster_source,"]
    #[sql = "  (SELECT i.path FROM show_images si JOIN show_image_candidates i ON i.id = si.image_id WHERE si.show_id = c.show_id AND si.kind = 1) AS poster_path"]
    #[sql = "FROM show_credits c JOIN shows s ON s.id = c.show_id WHERE c.person_id = ?"]
    list_person_show_credits: TypedStatement<(PersonId,), PersonCreditRow>,
    #[sql = "SELECT c.id, c.movie_id AS owner_id, m.default_language AS owner_language, c.credit_type, c.department, c.job, c.sort_order, c.episode_count, m.release_date AS date,"]
    #[sql = "  (SELECT i.source FROM movie_images mi JOIN movie_image_candidates i ON i.id = mi.image_id WHERE mi.movie_id = c.movie_id AND mi.kind = 1) AS poster_source,"]
    #[sql = "  (SELECT i.path FROM movie_images mi JOIN movie_image_candidates i ON i.id = mi.image_id WHERE mi.movie_id = c.movie_id AND mi.kind = 1) AS poster_path"]
    #[sql = "FROM movie_credits c JOIN movies m ON m.id = c.movie_id WHERE c.person_id = ?"]
    list_person_movie_credits: TypedStatement<(PersonId,), PersonCreditRow>,
    // Owner titles (kind=Title) and character names (kind=Character) for a person's credits.
    #[sql = "SELECT c.id, ss.language, ss.text FROM show_credits c"]
    #[sql = "JOIN show_strings ss ON ss.show_id = c.show_id AND ss.kind = 1 WHERE c.person_id = ? ORDER BY c.id"]
    list_person_show_titles: TypedStatement<(PersonId,), (CreditId, api::Locale, String)>,
    #[sql = "SELECT c.id, ms.language, ms.text FROM movie_credits c"]
    #[sql = "JOIN movie_strings ms ON ms.movie_id = c.movie_id AND ms.kind = 1 WHERE c.person_id = ? ORDER BY c.id"]
    list_person_movie_titles: TypedStatement<(PersonId,), (CreditId, api::Locale, String)>,
    #[sql = "SELECT cs.credit_id, cs.language, cs.text FROM show_credit_strings cs"]
    #[sql = "JOIN show_credits c ON c.id = cs.credit_id WHERE c.person_id = ? ORDER BY cs.credit_id"]
    list_person_show_credit_strings: TypedStatement<(PersonId,), (CreditId, api::Locale, String)>,
    #[sql = "SELECT cs.credit_id, cs.language, cs.text FROM movie_credit_strings cs"]
    #[sql = "JOIN movie_credits c ON c.id = cs.credit_id WHERE c.person_id = ? ORDER BY cs.credit_id"]
    list_person_movie_credit_strings: TypedStatement<(PersonId,), (CreditId, api::Locale, String)>,
    // Never-synced first, then stalest; a best-effort name for the task label.
    // Batch capped at 50 (PERSON_SYNC_BATCH) so a big cast drains gradually.
    #[sql = "SELECT p.id, (SELECT text FROM person_strings WHERE person_id = p.id AND kind = 1 LIMIT 1) AS name"]
    #[sql = "FROM people p"]
    #[sql = "WHERE p.last_synced_at IS NULL OR p.last_synced_at < ?"]
    #[sql = "ORDER BY p.last_synced_at IS NOT NULL, p.last_synced_at LIMIT 50"]
    people_needing_sync: TypedStatement<(Timestamp,), PersonSyncRow>,

    // selection tables
    #[sql = "SELECT si.kind, i.source, i.path, i.width, i.height, si.user_selected"]
    #[sql = "FROM show_images si JOIN show_image_candidates i ON i.id = si.image_id"]
    #[sql = "WHERE si.show_id = ?"]
    list_show_image_selections: TypedStatement<(ShowId,), ImageSelectionRow>,
    #[sql = "SELECT si.show_id, si.kind, i.source, i.path, i.width, i.height, si.user_selected"]
    #[sql = "FROM show_images si JOIN show_image_candidates i ON i.id = si.image_id"]
    list_all_show_image_selections: TypedStatement<(), AllShowImageSelectionRow>,
    #[sql = "SELECT mi.kind, i.source, i.path, i.width, i.height, mi.user_selected"]
    #[sql = "FROM movie_images mi JOIN movie_image_candidates i ON i.id = mi.image_id"]
    #[sql = "WHERE mi.movie_id = ?"]
    list_movie_image_selections: TypedStatement<(MovieId,), ImageSelectionRow>,
    #[sql = "SELECT mi.movie_id, mi.kind, i.source, i.path, i.width, i.height, mi.user_selected"]
    #[sql = "FROM movie_images mi JOIN movie_image_candidates i ON i.id = mi.image_id"]
    list_all_movie_image_selections: TypedStatement<(), AllMovieImageSelectionRow>,

    // seasons
    #[sql = "SELECT s.id, s.show_id, s.season, s.air_date,"]
    #[sql = "    i.source AS poster_source, i.path AS poster_path,"]
    #[sql = "    (SELECT COUNT(DISTINCT we.episode) FROM watched_episodes we WHERE we.user_id = ?1 AND we.show_id = s.show_id AND we.season = s.season) AS watched_count,"]
    #[sql = "    (SELECT COUNT(*) FROM episodes e WHERE e.show_id = s.show_id AND e.season = s.season) AS total_count"]
    #[sql = "FROM seasons s"]
    #[sql = "LEFT JOIN season_images si ON si.season_id = s.id AND si.kind = 1"]
    #[sql = "LEFT JOIN season_image_candidates i ON i.id = si.image_id"]
    #[sql = "WHERE s.show_id = ?2 ORDER BY s.season"]
    list_seasons: TypedStatement<(Option<UserId>, ShowId), SeasonRow>,
    #[sql = "SELECT ss.season_id, ss.language, ss.kind, ss.text FROM season_strings ss"]
    #[sql = "JOIN seasons s ON s.id = ss.season_id"]
    #[sql = "WHERE s.show_id = ? ORDER BY ss.season_id"]
    list_show_season_strings: TypedStatement<(ShowId,), AllSeasonStringRow>,
    #[sql = "SELECT episode FROM episodes WHERE show_id = ? AND season = ?"]
    episode_numbers_for_season: TypedStatement<(ShowId, SeasonNumber), u32>,

    // episodes
    #[sql = "SELECT show_id, season, episode FROM episodes WHERE id = ?"]
    episode_natural_key: TypedStatement<(EpisodeId,), EpisodeNaturalKeyRow>,
    #[sql = "SELECT id, season, episode FROM episodes WHERE show_id = ?"]
    list_episode_ids_for_show: TypedStatement<(ShowId,), EpisodeIdRow>,
    #[sql = "SELECT e.id, e.show_id, e.season, e.episode, e.absolute_number, e.aired, p.timestamp AS pending,"]
    #[sql = "    (SELECT COUNT(*) FROM watched_episodes we WHERE we.user_id = ?1 AND we.show_id = e.show_id AND we.season = e.season AND we.episode = e.episode) AS watched_count"]
    #[sql = "FROM episodes e"]
    #[sql = "LEFT JOIN pending p ON p.episode_id = e.id AND p.user_id = ?1"]
    #[sql = "WHERE e.show_id = ?2 AND e.season = ?3"]
    #[sql = "ORDER BY e.episode"]
    list_episodes: TypedStatement<(Option<UserId>, ShowId, SeasonNumber), EpisodeRow>,
    #[sql = "SELECT e.id, e.show_id, e.season, e.episode, e.absolute_number, e.aired, p.timestamp AS pending,"]
    #[sql = "    (SELECT COUNT(*) FROM watched_episodes we WHERE we.user_id = ?1 AND we.show_id = e.show_id AND we.season = e.season AND we.episode = e.episode) AS watched_count"]
    #[sql = "FROM episodes e"]
    #[sql = "LEFT JOIN pending p ON p.episode_id = e.id AND p.user_id = ?1"]
    #[sql = "WHERE e.id = ?2"]
    episode_by_id: TypedStatement<(Option<UserId>, EpisodeId), EpisodeRow>,
    #[sql = "SELECT es.episode_id, es.language, es.kind, es.text FROM episode_strings es"]
    #[sql = "JOIN episodes e ON e.id = es.episode_id"]
    #[sql = "WHERE e.show_id = ? AND e.season = ? ORDER BY es.episode_id"]
    list_season_episode_strings: TypedStatement<(ShowId, SeasonNumber), AllEpisodeStringRow>,
    #[sql = "SELECT we.id, we.timestamp, we.season, we.episode, e.id AS episode_id"]
    #[sql = "FROM watched_episodes we"]
    #[sql = "JOIN episodes e ON e.show_id = we.show_id AND e.season = we.season AND e.episode = we.episode"]
    #[sql = "WHERE we.user_id = ? AND we.show_id = ?"]
    #[sql = "ORDER BY we.timestamp DESC"]
    list_episodes_watched: TypedStatement<(UserId, ShowId), WatchedEpisodeRow>,

    // slim list views
    #[sql = "SELECT id, release_date AS date, EXISTS (SELECT 1 FROM user_tracked_movies t WHERE t.movie_id = movies.id AND t.user_id = ?1) AS tracked, (SELECT c.value FROM user_movie_config c WHERE c.user_id = ?1 AND c.movie_id = movies.id AND c.key = 'language') AS language, default_language FROM movies ORDER BY id"]
    list_movie_items: TypedStatement<(UserId,), MediaItemRow>,
    #[sql = "SELECT id, first_air AS date, EXISTS (SELECT 1 FROM user_tracked_shows t WHERE t.show_id = shows.id AND t.user_id = ?1) AS tracked, (SELECT c.value FROM user_show_config c WHERE c.user_id = ?1 AND c.show_id = shows.id AND c.key = 'language') AS language, default_language FROM shows ORDER BY id"]
    list_show_items: TypedStatement<(UserId,), MediaItemRow>,
    #[sql = "SELECT movie_id, MAX(timestamp) AS last_watched FROM watched_movies"]
    #[sql = "WHERE user_id = ? AND movie_id IS NOT NULL GROUP BY movie_id"]
    last_watched_movies: TypedStatement<(UserId,), LastWatchedMovieRow>,
    #[sql = "SELECT show_id, MAX(timestamp) AS last_watched FROM watched_episodes WHERE user_id = ? GROUP BY show_id"]
    last_watched_shows: TypedStatement<(UserId,), LastWatchedShowRow>,
    #[sql = "SELECT e.show_id AS show_id, COUNT(*) AS unwatched FROM episodes e WHERE e.season != 0 AND e.aired IS NOT NULL AND e.aired <= CAST(strftime('%s', 'now') AS INTEGER) * 1000 AND NOT EXISTS (SELECT 1 FROM watched_episodes w WHERE w.user_id = ?1 AND w.show_id = e.show_id AND w.season = e.season AND w.episode = e.episode) GROUP BY e.show_id"]
    unwatched_shows: TypedStatement<(UserId,), UnwatchedShowRow>,
    // The next episode in watch order per show and scope (specials or regular
    // seasons): the first aired episode after the most recent watch in that
    // scope, or the first aired one with no watch. ?2 limits it to one show.
    #[sql = "WITH last AS ("]
    #[sql = "    SELECT show_id, season = 0 AS special, season, episode,"]
    #[sql = "        ROW_NUMBER() OVER (PARTITION BY show_id, season = 0 ORDER BY timestamp DESC, id DESC) AS rn"]
    #[sql = "    FROM watched_episodes WHERE user_id = ?1 AND show_id IS NOT NULL AND (?2 IS NULL OR show_id = ?2)"]
    #[sql = "), next AS ("]
    #[sql = "    SELECT e.show_id, e.season = 0 AS special, e.id AS episode_id,"]
    #[sql = "        ROW_NUMBER() OVER (PARTITION BY e.show_id, e.season = 0 ORDER BY e.season, e.episode) AS rn"]
    #[sql = "    FROM episodes e"]
    #[sql = "    LEFT JOIN last l ON l.rn = 1 AND l.show_id = e.show_id AND l.special = (e.season = 0)"]
    #[sql = "    WHERE (?2 IS NULL OR e.show_id = ?2) AND e.aired IS NOT NULL AND e.aired <= ?3"]
    #[sql = "        AND (l.show_id IS NULL OR (e.season, e.episode) > (l.season, l.episode))"]
    #[sql = ")"]
    #[sql = "SELECT show_id, special, episode_id FROM next WHERE rn = 1"]
    next_episodes: TypedStatement<(UserId, Option<ShowId>, Timestamp), WatchOrderNextRow>,

    // movies
    #[sql = "SELECT m.id, m.release_date, EXISTS (SELECT 1 FROM user_tracked_movies t WHERE t.movie_id = m.id AND t.user_id = ?1) AS tracked, m.auto_sync, m.last_synced_at, (SELECT c.value FROM user_movie_config c WHERE c.user_id = ?1 AND c.movie_id = m.id AND c.key = 'language') AS language, m.default_language, m.release_filters"]
    #[sql = "FROM movies m ORDER BY m.id"]
    list_movies: TypedStatement<(Option<UserId>,), MovieRow>,
    #[sql = "SELECT m.id, m.release_date, EXISTS (SELECT 1 FROM user_tracked_movies t WHERE t.movie_id = m.id AND t.user_id = ?1) AS tracked, m.auto_sync, m.last_synced_at, (SELECT c.value FROM user_movie_config c WHERE c.user_id = ?1 AND c.movie_id = m.id AND c.key = 'language') AS language, m.default_language, m.release_filters"]
    #[sql = "FROM movies m WHERE m.id = ?2"]
    movie_by_id: TypedStatement<(Option<UserId>, MovieId), MovieRow>,
    #[sql = "SELECT release_filters FROM movies WHERE id = ?"]
    movie_release_filters: TypedStatement<(MovieId,), Option<String>>,
    #[sql = "SELECT m.id, m.release_date, EXISTS (SELECT 1 FROM user_tracked_movies t WHERE t.movie_id = m.id AND t.user_id = ?1) AS tracked, m.auto_sync, m.last_synced_at, (SELECT c.value FROM user_movie_config c WHERE c.user_id = ?1 AND c.movie_id = m.id AND c.key = 'language') AS language, m.default_language, m.release_filters"]
    #[sql = "FROM movies m"]
    #[sql = "JOIN movie_remotes r ON r.movie_id = m.id"]
    #[sql = "WHERE r.source = ?2 AND r.value = ?3"]
    movie_by_remote: TypedStatement<(Option<UserId>, RemoteSource, RemoteValue), MovieRow>,
    #[sql = "SELECT id, slug, source, value, enabled, priority, sync_kinds, cache FROM movie_remotes WHERE movie_id = ? ORDER BY priority, id"]
    list_movie_remotes: TypedStatement<(MovieId,), RemoteRow>,
    #[sql = "SELECT movie_id, id, slug, source, value, enabled, priority, sync_kinds, cache FROM movie_remotes ORDER BY movie_id, priority, id"]
    list_all_movie_remotes: TypedStatement<(), AllMovieRemoteRow>,
    #[sql = "SELECT movie_id, language, kind, text FROM movie_strings ORDER BY movie_id"]
    list_all_movie_strings: TypedStatement<(), AllMovieStringRow>,
    #[sql = "SELECT movie_id FROM movie_remotes WHERE source = ? AND value = ? LIMIT 1"]
    movie_id_by_remote: TypedStatement<(RemoteSource, RemoteValue), Option<MovieId>>,
    #[sql = "SELECT release_date FROM movies WHERE id = ?"]
    movie_released_by_id: TypedStatement<(MovieId,), Option<Timestamp>>,

    // watched
    #[sql = "SELECT we.id, we.timestamp, e.id AS episode_id, NULL AS movie_id, e.show_id"]
    #[sql = "FROM watched_episodes we"]
    #[sql = "JOIN episodes e ON e.show_id = we.show_id AND e.season = we.season AND e.episode = we.episode"]
    #[sql = "WHERE we.user_id = ? AND e.id = ?"]
    #[sql = "ORDER BY we.timestamp DESC"]
    list_watched_by_episode: TypedStatement<(UserId, EpisodeId), WatchedRow>,
    #[sql = "SELECT id, timestamp, NULL AS episode_id, movie_id, NULL AS show_id"]
    #[sql = "FROM watched_movies WHERE user_id = ? AND movie_id = ? ORDER BY timestamp DESC"]
    list_watched_by_movie: TypedStatement<(UserId, MovieId), WatchedRow>,
    #[sql = "SELECT we.id, we.timestamp, we.show_id, we.season, we.episode"]
    #[sql = "FROM watched_episodes we"]
    #[sql = "LEFT JOIN episodes e"]
    #[sql = "    ON e.show_id = we.show_id AND e.season = we.season AND e.episode = we.episode"]
    #[sql = "WHERE we.user_id = ? AND we.show_id = ? AND e.id IS NULL"]
    #[sql = "ORDER BY we.timestamp ASC"]
    list_orphaned_for_show: TypedStatement<(UserId, ShowId), OrphanedWatchedRow>,
    // select episodes which have 0 watched by show and season.
    #[sql = "SELECT id, show_id, season, episode FROM episodes"]
    #[sql = "WHERE show_id = ?2 AND season = ?3"]
    #[sql = "    AND NOT EXISTS ("]
    #[sql = "        SELECT 1 FROM watched_episodes we"]
    #[sql = "        WHERE we.user_id = ?1"]
    #[sql = "        AND we.show_id = episodes.show_id"]
    #[sql = "        AND we.season = episodes.season"]
    #[sql = "        AND we.episode = episodes.episode"]
    #[sql = "    )"]
    select_unwatched_by_show_season:
        TypedStatement<(UserId, ShowId, SeasonNumber), UnwatchedEpisodeRow>,

    // pending table management
    #[sql = "SELECT timestamp FROM pending WHERE user_id = ? AND movie_id = ? LIMIT 1"]
    select_pending_movie: TypedStatement<(Option<UserId>, MovieId), Timestamp>,
    #[sql = "SELECT 1 FROM pending WHERE user_id = ? AND show_id = ? LIMIT 1"]
    has_pending_episode_for_show: TypedStatement<(UserId, ShowId), (i64,)>,
    #[sql = "SELECT e.aired, p.timestamp"]
    #[sql = "FROM pending p"]
    #[sql = "JOIN episodes e ON e.id = p.episode_id"]
    #[sql = "WHERE p.user_id = ? AND p.show_id = ?"]
    pending_episode_aired_for_show: TypedStatement<(UserId, ShowId), PendingEpisodeAiredRow>,
    #[sql = "SELECT e.season, e.episode"]
    #[sql = "FROM episodes e"]
    #[sql = "WHERE e.show_id = ?1"]
    #[sql = "    AND e.aired IS NOT NULL"]
    #[sql = "    AND e.season <> 0"]
    #[sql = "ORDER BY ABS(e.aired - ?2)"]
    #[sql = "LIMIT 1"]
    find_episode_by_timestamp: TypedStatement<(ShowId, Timestamp), EpisodeMatchRow>,
    #[sql = "SELECT e.id, e.aired"]
    #[sql = "FROM episodes e"]
    #[sql = "WHERE e.show_id = ?2"]
    #[sql = "    AND e.aired IS NOT NULL"]
    #[sql = "    AND (?3 OR e.season <> 0)"]
    #[sql = "    AND NOT EXISTS ("]
    #[sql = "        SELECT 1 FROM watched_episodes we"]
    #[sql = "        WHERE we.user_id = ?1 AND we.show_id = e.show_id AND we.season = e.season AND we.episode = e.episode"]
    #[sql = "    )"]
    #[sql = "ORDER BY e.season, e.episode"]
    #[sql = "LIMIT 1"]
    next_pending_episode_for_show:
        TypedStatement<(UserId, ShowId, api::IncludeSpecials), NextEpisodeRow>,
    #[cfg(feature = "import")]
    #[sql = "SELECT e.id, e.aired"]
    #[sql = "FROM episodes e"]
    #[sql = "WHERE e.show_id = ?2"]
    #[sql = "    AND NOT EXISTS ("]
    #[sql = "        SELECT 1 FROM watched_episodes we"]
    #[sql = "        WHERE we.user_id = ?1 AND we.show_id = e.show_id AND we.season = e.season AND we.episode = e.episode"]
    #[sql = "    )"]
    #[sql = "ORDER BY e.season, e.episode"]
    #[sql = "LIMIT 1"]
    first_unwatched_episode_for_show: TypedStatement<(UserId, ShowId), NextEpisodeRow>,
    #[sql = "SELECT t.user_id, m.id, m.release_filters"]
    #[sql = "FROM user_tracked_movies t"]
    #[sql = "JOIN movies m ON m.id = t.movie_id"]
    #[sql = "WHERE NOT EXISTS (SELECT 1 FROM watched_movies wm WHERE wm.user_id = t.user_id AND wm.movie_id = m.id)"]
    #[sql = "    AND NOT EXISTS (SELECT 1 FROM pending p WHERE p.user_id = t.user_id AND p.movie_id = m.id)"]
    movie_pending_candidates: TypedStatement<(), MoviePendingCandidateRow>,
    #[sql = "SELECT 1 FROM watched_movies WHERE user_id = ? AND movie_id = ? LIMIT 1"]
    has_watched_movie: TypedStatement<(UserId, MovieId), (i64,)>,
    #[sql = "SELECT episode_id, movie_id, timestamp"]
    #[sql = "FROM pending"]
    #[sql = "WHERE user_id = ? AND timestamp <= ?"]
    #[sql = "ORDER BY timestamp DESC"]
    list_pending_before: TypedStatement<(UserId, Timestamp), PendingBaseRow>,
    #[sql = "SELECT timestamp FROM pending WHERE user_id = ? AND episode_id = ?"]
    pending_timestamp_for_episode: TypedStatement<(UserId, EpisodeId), (Timestamp,)>,
    #[sql = "SELECT timestamp FROM pending WHERE user_id = ? AND movie_id = ?"]
    pending_timestamp_for_movie: TypedStatement<(UserId, MovieId), (Timestamp,)>,
    #[sql = "SELECT episode_id, timestamp FROM pending WHERE user_id = ? AND show_id = ?"]
    pending_for_show: TypedStatement<(UserId, ShowId), (EpisodeId, Timestamp)>,
    #[sql = "SELECT e.show_id, (SELECT c.value FROM user_show_config c WHERE c.user_id = ?1 AND c.show_id = s.id AND c.key = 'language') AS language, s.default_language, e.season, e.episode, e.aired"]
    #[sql = "FROM episodes e"]
    #[sql = "JOIN shows s ON s.id = e.show_id"]
    #[sql = "JOIN user_tracked_shows t ON t.show_id = s.id AND t.user_id = ?1"]
    #[sql = "WHERE e.id = ?2"]
    pending_episode_detail: TypedStatement<(UserId, EpisodeId), PendingEpisodeDetailRow>,
    #[sql = "SELECT m.release_date FROM movies m"]
    #[sql = "JOIN user_tracked_movies t ON t.movie_id = m.id AND t.user_id = ?1"]
    #[sql = "WHERE m.id = ?2"]
    pending_movie_detail: TypedStatement<(UserId, MovieId), PendingMovieDetailRow>,
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
    #[sql = "SELECT e.show_id, e.season, e.episode, e.aired"]
    #[sql = "FROM episodes e"]
    #[sql = "JOIN user_tracked_shows t ON t.show_id = e.show_id"]
    #[sql = "WHERE t.user_id = ?"]
    #[sql = "    AND e.aired >= ?"]
    #[sql = "    AND e.aired < ?"]
    #[sql = "ORDER BY e.aired, e.show_id, e.season, e.episode"]
    list_schedule: TypedStatement<(UserId, Timestamp, Timestamp), ScheduleRow>,
    #[sql = "SELECT m.id, m.release_date"]
    #[sql = "FROM movies m"]
    #[sql = "JOIN user_tracked_movies t ON t.movie_id = m.id"]
    #[sql = "WHERE t.user_id = ?"]
    #[sql = "    AND m.release_date >= ?"]
    #[sql = "    AND m.release_date < ?"]
    #[sql = "ORDER BY m.release_date, m.id"]
    list_schedule_movies: TypedStatement<(UserId, Timestamp, Timestamp), ScheduleMovieRow>,

    // all watched + existence checks (backup export/import)
    #[sql = "SELECT w.id, u.login, w.timestamp, w.show_id, w.season, w.episode FROM watched_episodes w"]
    #[sql = "JOIN users u ON u.id = w.user_id"]
    #[sql = "ORDER BY u.login, w.show_id, w.season, w.episode, w.id"]
    list_all_watched_episodes: TypedStatement<(), AllWatchedEpisodeRow>,
    #[sql = "SELECT w.id, u.login, w.timestamp, w.movie_id FROM watched_movies w"]
    #[sql = "JOIN users u ON u.id = w.user_id"]
    #[sql = "ORDER BY u.login, w.movie_id, w.id"]
    list_all_watched_movies: TypedStatement<(), AllWatchedMovieRow>,
    #[sql = "SELECT 1 FROM watched_episodes WHERE id = ? LIMIT 1"]
    watched_episode_exists: TypedStatement<(WatchedId,), (i64,)>,
    #[sql = "SELECT 1 FROM watched_movies WHERE id = ? LIMIT 1"]
    watched_movie_exists: TypedStatement<(WatchedId,), (i64,)>,
    #[sql = "BEGIN"]
    begin: TypedStatement<(), ()>,
    #[sql = "COMMIT"]
    commit: TypedStatement<(), ()>,
    #[sql = "SELECT 1 FROM show_remotes WHERE id = ? OR (show_id = ? AND source = ? AND value = ?) LIMIT 1"]
    show_remote_exists: TypedStatement<(RemoteId, ShowId, RemoteSource, RemoteValue), (i64,)>,
    #[sql = "SELECT 1 FROM movie_remotes WHERE id = ? OR (movie_id = ? AND source = ? AND value = ?) LIMIT 1"]
    movie_remote_exists: TypedStatement<(RemoteId, MovieId, RemoteSource, RemoteValue), (i64,)>,
    #[sql = "SELECT u.login, t.show_id FROM user_tracked_shows t"]
    #[sql = "JOIN users u ON u.id = t.user_id ORDER BY u.login, t.show_id"]
    list_all_tracked_shows: TypedStatement<(), (String, ShowId)>,
    #[sql = "SELECT u.login, t.movie_id FROM user_tracked_movies t"]
    #[sql = "JOIN users u ON u.id = t.user_id ORDER BY u.login, t.movie_id"]
    list_all_tracked_movies: TypedStatement<(), (String, MovieId)>,
    #[sql = "SELECT u.login, c.key, c.value FROM user_config c"]
    #[sql = "JOIN users u ON u.id = c.user_id ORDER BY u.login, c.key"]
    list_all_user_config: TypedStatement<(), (String, String, String)>,
    #[sql = "SELECT u.login, c.show_id, c.key, c.value FROM user_show_config c"]
    #[sql = "JOIN users u ON u.id = c.user_id ORDER BY u.login, c.show_id, c.key"]
    list_all_user_show_config: TypedStatement<(), (String, ShowId, String, String)>,
    #[sql = "SELECT u.login, c.movie_id, c.key, c.value FROM user_movie_config c"]
    #[sql = "JOIN users u ON u.id = c.user_id ORDER BY u.login, c.movie_id, c.key"]
    list_all_user_movie_config: TypedStatement<(), (String, MovieId, String, String)>,

    // config
    #[sql = "SELECT value FROM config WHERE key = ?"]
    get_config: TypedStatement<(String,), String>,
    #[sql = "SELECT key, value FROM user_config WHERE user_id = ?"]
    list_user_config: TypedStatement<(UserId,), (String, String)>,
    #[sql = "SELECT value FROM user_config WHERE user_id = ? AND key = 'language'"]
    user_language: TypedStatement<(Option<UserId>,), Pref<api::Locale>>,
    // The user's include-specials for the show, then their global one.
    #[sql = "SELECT (SELECT value FROM user_show_config WHERE user_id = ?1 AND show_id = ?2 AND key = 'include-specials'),"]
    #[sql = "    (SELECT value FROM user_config WHERE user_id = ?1 AND key = 'include-specials')"]
    user_include_specials: TypedStatement<(UserId, ShowId), (Pref<IncludeSpecials>, Pref<bool>)>,
    // Users who track the show or picked a language for it, with the language
    // they see it in (their show language, else their global one).
    #[sql = "SELECT (SELECT value FROM user_show_config c WHERE c.user_id = u.id AND c.show_id = ?1 AND c.key = 'language'),"]
    #[sql = "    (SELECT value FROM user_config c WHERE c.user_id = u.id AND c.key = 'language')"]
    #[sql = "FROM users u"]
    #[sql = "WHERE EXISTS (SELECT 1 FROM user_tracked_shows t WHERE t.user_id = u.id AND t.show_id = ?1)"]
    #[sql = "    OR EXISTS (SELECT 1 FROM user_show_config c WHERE c.user_id = u.id AND c.show_id = ?1 AND c.key = 'language')"]
    show_viewer_languages: TypedStatement<(ShowId,), (Pref<api::Locale>, Pref<api::Locale>)>,
    #[sql = "SELECT (SELECT value FROM user_movie_config c WHERE c.user_id = u.id AND c.movie_id = ?1 AND c.key = 'language'),"]
    #[sql = "    (SELECT value FROM user_config c WHERE c.user_id = u.id AND c.key = 'language')"]
    #[sql = "FROM users u"]
    #[sql = "WHERE EXISTS (SELECT 1 FROM user_tracked_movies t WHERE t.user_id = u.id AND t.movie_id = ?1)"]
    #[sql = "    OR EXISTS (SELECT 1 FROM user_movie_config c WHERE c.user_id = u.id AND c.movie_id = ?1 AND c.key = 'language')"]
    movie_viewer_languages: TypedStatement<(MovieId,), (Pref<api::Locale>, Pref<api::Locale>)>,

    // derived state (recomputed periodically)
    #[sql = "SELECT top_languages FROM state WHERE id = 0"]
    get_state_top_languages: TypedStatement<(), String>,
    #[sql = "SELECT value AS language FROM user_show_config WHERE key = 'language'"]
    list_show_languages: TypedStatement<(), LanguageRow>,
    #[sql = "SELECT value AS language FROM user_movie_config WHERE key = 'language'"]
    list_movie_languages: TypedStatement<(), LanguageRow>,

    // stale-item queries
    #[sql = "SELECT shows.id, first_air, EXISTS (SELECT 1 FROM user_tracked_shows t WHERE t.show_id = shows.id) AS tracked, auto_sync, last_synced_at, NULL AS language, default_language, NULL AS include_specials, air_date_filters"]
    #[sql = "FROM shows"]
    #[sql = "WHERE auto_sync = 1"]
    #[sql = "    AND (last_synced_at IS NULL OR last_synced_at < ?)"]
    #[sql = "ORDER BY last_synced_at IS NOT NULL, last_synced_at"]
    shows_needing_sync: TypedStatement<(Timestamp,), ShowRow>,
    #[sql = "SELECT m.id, m.release_date, EXISTS (SELECT 1 FROM user_tracked_movies t WHERE t.movie_id = m.id) AS tracked, m.auto_sync, m.last_synced_at, NULL AS language, m.default_language, m.release_filters"]
    #[sql = "FROM movies m"]
    #[sql = "WHERE m.auto_sync = 1"]
    #[sql = "    AND (m.last_synced_at IS NULL OR m.last_synced_at < ?)"]
    #[sql = "ORDER BY m.last_synced_at IS NOT NULL, m.last_synced_at"]
    movies_needing_sync: TypedStatement<(Timestamp,), MovieRow>,
    // Episodes inside the window around their air date, due another hourly sync.
    // Bound as (window start, window end, sync cutoff).
    #[sql = "SELECT e.id, e.show_id, e.season, e.episode"]
    #[sql = "FROM episodes e JOIN shows s ON s.id = e.show_id"]
    #[sql = "WHERE s.auto_sync = 1"]
    #[sql = "    AND EXISTS (SELECT 1 FROM user_tracked_shows t WHERE t.show_id = s.id)"]
    #[sql = "    AND e.aired IS NOT NULL"]
    #[sql = "    AND e.aired BETWEEN ? AND ?"]
    #[sql = "    AND (e.last_synced_at IS NULL OR e.last_synced_at < ?)"]
    #[sql = "ORDER BY e.last_synced_at IS NOT NULL, e.last_synced_at"]
    episodes_needing_air_sync: TypedStatement<(Timestamp, Timestamp, Timestamp), EpisodeAirSyncRow>,

    // per-episode conditional-request state
    #[sql = "SELECT source, cache FROM episode_cache WHERE episode_id = ?"]
    episode_cache: TypedStatement<(EpisodeId,), (RemoteSource, String)>,

    // movie releases
    #[sql = "SELECT source, country, release_type, timestamp"]
    #[sql = "FROM movie_releases"]
    #[sql = "WHERE movie_id = ?"]
    #[sql = "ORDER BY timestamp, source, country, release_type"]
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
    #[sql = "SELECT source, country, network, timestamp"]
    #[sql = "FROM episode_releases"]
    #[sql = "WHERE episode_id = ?"]
    #[sql = "ORDER BY timestamp, source, country, network"]
    list_episode_releases: TypedStatement<(EpisodeId,), (RemoteSource, Country, String, Timestamp)>,

    // translated strings (per entity)
    #[sql = "SELECT language, kind, text FROM season_strings WHERE season_id = ? ORDER BY kind, language"]
    list_season_strings: TypedStatement<(SeasonId,), (api::Locale, api::StringKind, String)>,
}

#[derive(Statements)]
pub(crate) struct InnerWrite {
    #[sql(statements)]
    read: InnerRead,
    #[sql(statements)]
    users_write: users::Write,

    #[sql = "BEGIN IMMEDIATE"]
    begin_immediate: TypedStatement<(), ()>,
    #[sql = "ROLLBACK"]
    rollback: TypedStatement<(), ()>,

    // shows
    #[sql = "INSERT INTO shows (id, first_air)"]
    #[sql = "VALUES (?, ?)"]
    insert_show: TypedStatement<(ShowId, Option<Timestamp>), ()>,
    #[sql = "UPDATE shows"]
    #[sql = "SET first_air = ?"]
    #[sql = "WHERE id = ?"]
    update_show: TypedStatement<(Option<Timestamp>, ShowId), ()>,
    #[sql = "UPDATE shows SET air_date_filters = ? WHERE id = ?"]
    update_show_air_date_filters: TypedStatement<(Option<String>, ShowId), ()>,
    #[sql = "DELETE FROM shows WHERE id = ?"]
    delete_show: TypedStatement<(ShowId,), ()>,
    #[sql = "INSERT OR IGNORE INTO user_tracked_shows (user_id, show_id) VALUES (?, ?)"]
    track_show: TypedStatement<(UserId, ShowId), ()>,
    #[sql = "DELETE FROM user_tracked_shows WHERE user_id = ? AND show_id = ?"]
    untrack_show: TypedStatement<(UserId, ShowId), ()>,
    #[sql = "UPDATE shows SET auto_sync = ? WHERE id = ?"]
    set_show_auto_sync: TypedStatement<(bool, ShowId), ()>,

    // remotes (one table per owner; source is a numeric enum, value is dynamic)
    #[sql = "INSERT INTO show_remotes (id, slug, show_id, source, value, enabled, priority, sync_kinds) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(show_id, source, value) DO UPDATE SET slug = COALESCE(excluded.slug, slug)"]
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
    #[sql = "UPDATE show_remotes SET cache = ? WHERE id = ?"]
    set_show_remote_cache: TypedStatement<(Option<String>, RemoteId), ()>,

    // images (shows and movies share one table)
    #[sql = "DELETE FROM show_image_candidates WHERE show_id = ?"]
    delete_show_images: TypedStatement<(ShowId,), ()>,
    #[sql = "DELETE FROM show_image_candidates WHERE show_id = ? AND source = ?"]
    delete_show_images_for_source: TypedStatement<(ShowId, ImageSource), ()>,
    #[sql = "SELECT kind, MAX(rank) AS rank FROM show_image_candidates WHERE show_id = ? GROUP BY kind"]
    show_image_max_ranks: TypedStatement<(ShowId,), KindRankRow>,
    #[sql = "SELECT kind FROM show_images WHERE show_id = ?"]
    show_selected_image_kinds: TypedStatement<(ShowId,), ImageKind>,
    #[sql = "INSERT INTO show_image_candidates (id, show_id, kind, source, path, width, height, rank, score) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"]
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
            Option<f64>,
        ),
        (),
    >,
    #[sql = "INSERT INTO episode_image_candidates (id, episode_id, kind, source, path, width, height) VALUES (?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(episode_id, kind, path) WHERE episode_id IS NOT NULL DO NOTHING"]
    insert_episode_image:
        TypedStatement<(ImageId, EpisodeId, ImageKind, ImageSource, String, u32, u32), ()>,
    #[sql = "DELETE FROM episode_image_candidates WHERE episode_id IN (SELECT id FROM episodes WHERE show_id = ?)"]
    delete_episode_images_for_show: TypedStatement<(ShowId,), ()>,
    #[sql = "DELETE FROM episode_image_candidates WHERE episode_id = ?"]
    delete_images_for_episode: TypedStatement<(EpisodeId,), ()>,
    #[sql = "DELETE FROM movie_image_candidates WHERE movie_id = ?"]
    delete_movie_images: TypedStatement<(MovieId,), ()>,
    #[sql = "INSERT INTO movie_image_candidates (id, movie_id, kind, source, path, width, height, rank, score) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"]
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
            Option<f64>,
        ),
        (),
    >,
    #[sql = "DELETE FROM season_images WHERE season_id = ? AND kind = ?"]
    delete_season_image_selection: TypedStatement<(SeasonId, ImageKind), ()>,

    // selection tables
    #[sql = "INSERT OR REPLACE INTO show_images (show_id, kind, image_id, user_selected) VALUES (?, ?, ?, ?)"]
    set_show_image_selection: TypedStatement<(ShowId, ImageKind, ImageId, bool), ()>,
    #[sql = "DELETE FROM show_images WHERE show_id = ? AND kind = ?"]
    delete_show_image_selection: TypedStatement<(ShowId, ImageKind), ()>,
    // Kind + (source, path) of the show's user-chosen selections, for preserving
    // them across a sync that clears and re-inserts image rows.
    #[sql = "SELECT si.kind, i.source, i.path FROM show_images si"]
    #[sql = "JOIN show_image_candidates i ON i.id = si.image_id"]
    #[sql = "WHERE si.show_id = ? AND si.user_selected = 1"]
    user_selected_show_images: TypedStatement<(ShowId,), UserSelectedRow>,
    // Lowest-rank (highest-priority remote, top score) image id for a kind.
    #[sql = "SELECT id FROM show_image_candidates WHERE show_id = ? AND kind = ? ORDER BY rank ASC LIMIT 1"]
    best_show_image: TypedStatement<(ShowId, ImageKind), ImageId>,
    // Distinct kinds with at least one stored image for a show.
    #[sql = "SELECT DISTINCT kind FROM show_image_candidates WHERE show_id = ?"]
    show_image_kinds: TypedStatement<(ShowId,), ImageKind>,
    #[sql = "INSERT OR REPLACE INTO movie_images (movie_id, kind, image_id, user_selected) VALUES (?, ?, ?, ?)"]
    set_movie_image_selection: TypedStatement<(MovieId, ImageKind, ImageId, bool), ()>,
    #[sql = "DELETE FROM movie_images WHERE movie_id = ? AND kind = ?"]
    delete_movie_image_selection: TypedStatement<(MovieId, ImageKind), ()>,
    #[sql = "SELECT si.kind, i.source, i.path FROM movie_images si"]
    #[sql = "JOIN movie_image_candidates i ON i.id = si.image_id"]
    #[sql = "WHERE si.movie_id = ? AND si.user_selected = 1"]
    user_selected_movie_images: TypedStatement<(MovieId,), UserSelectedRow>,
    #[sql = "SELECT id FROM movie_image_candidates WHERE movie_id = ? AND kind = ? ORDER BY rank ASC LIMIT 1"]
    best_movie_image: TypedStatement<(MovieId, ImageKind), ImageId>,
    #[sql = "SELECT DISTINCT kind FROM movie_image_candidates WHERE movie_id = ?"]
    movie_image_kinds: TypedStatement<(MovieId,), ImageKind>,
    #[sql = "INSERT OR REPLACE INTO episode_images (episode_id, kind, image_id) VALUES (?, ?, ?)"]
    set_episode_image_selection: TypedStatement<(EpisodeId, ImageKind, ImageId), ()>,
    #[sql = "DELETE FROM season_image_candidates WHERE season_id = ?"]
    delete_season_images: TypedStatement<(SeasonId,), ()>,
    #[sql = "INSERT INTO season_image_candidates (id, season_id, kind, source, path, width, height, rank) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
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

    // people & credits
    #[sql = "SELECT pr.person_id, p.last_synced_at FROM person_remotes pr"]
    #[sql = "JOIN people p ON p.id = pr.person_id WHERE pr.source = ? AND pr.value = ?"]
    person_by_remote: TypedStatement<(RemoteSource, RemoteValue), (PersonId, Option<Timestamp>)>,
    #[sql = "INSERT INTO people (id) VALUES (?)"]
    insert_person: TypedStatement<(PersonId,), ()>,
    #[sql = "UPDATE people SET department = ?, default_language = ? WHERE id = ?"]
    update_person: TypedStatement<(Option<String>, api::Locale, PersonId), ()>,
    #[sql = "UPDATE people SET last_synced_at = ? WHERE id = ?"]
    mark_person_synced: TypedStatement<(Timestamp, PersonId), ()>,
    #[sql = "UPDATE people SET remote_id = ? WHERE id = ?"]
    set_person_primary_remote: TypedStatement<(RemoteId, PersonId), ()>,
    // person_remotes (identical shape to show_remotes/movie_remotes)
    #[sql = "INSERT INTO person_remotes (id, slug, person_id, source, value, enabled, priority, sync_kinds) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(person_id, source, value) DO UPDATE SET slug = COALESCE(excluded.slug, slug)"]
    insert_person_remote: TypedStatement<
        (
            RemoteId,
            Option<String>,
            PersonId,
            RemoteSource,
            RemoteValue,
            bool,
            i32,
            Option<api::SyncKindSet>,
        ),
        (),
    >,
    #[sql = "DELETE FROM person_remotes WHERE id = ?"]
    delete_person_remote: TypedStatement<(RemoteId,), ()>,
    #[sql = "UPDATE person_remotes SET slug = ?, source = ?, value = ? WHERE id = ?"]
    update_person_remote: TypedStatement<(Option<String>, RemoteSource, RemoteValue, RemoteId), ()>,
    #[sql = "UPDATE person_remotes SET enabled = ? WHERE id = ?"]
    set_person_remote_enabled: TypedStatement<(bool, RemoteId), ()>,
    #[sql = "UPDATE person_remotes SET priority = ? WHERE id = ?"]
    set_person_remote_priority: TypedStatement<(i32, RemoteId), ()>,
    #[sql = "UPDATE person_remotes SET sync_kinds = ? WHERE id = ?"]
    set_person_remote_sync_kinds: TypedStatement<(Option<api::SyncKindSet>, RemoteId), ()>,
    #[sql = "UPDATE person_remotes SET cache = ? WHERE id = ?"]
    set_person_remote_cache: TypedStatement<(Option<String>, RemoteId), ()>,
    #[sql = "DELETE FROM person_remotes WHERE person_id = ?"]
    delete_person_remotes: TypedStatement<(PersonId,), ()>,
    #[sql = "DELETE FROM people WHERE id = ?"]
    delete_person: TypedStatement<(PersonId,), ()>,
    #[sql = "DELETE FROM person_strings WHERE person_id = ?"]
    clear_person_strings: TypedStatement<(PersonId,), ()>,
    #[sql = "INSERT OR IGNORE INTO person_strings (person_id, language, kind, text) VALUES (?, ?, ?, ?)"]
    insert_person_string: TypedStatement<(PersonId, api::Locale, api::StringKind, String), ()>,
    #[sql = "DELETE FROM person_image_candidates WHERE person_id = ?"]
    clear_person_images: TypedStatement<(PersonId,), ()>,
    #[sql = "INSERT INTO person_image_candidates (id, person_id, kind, source, path, width, height, rank, score) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(person_id, kind, path) DO NOTHING"]
    insert_person_image: TypedStatement<
        (
            ImageId,
            PersonId,
            ImageKind,
            ImageSource,
            String,
            u32,
            u32,
            u32,
            Option<f64>,
        ),
        (),
    >,
    #[sql = "DELETE FROM show_credits WHERE show_id = ?"]
    clear_show_credits: TypedStatement<(ShowId,), ()>,
    #[sql = "INSERT INTO show_credits (id, show_id, person_id, credit_type, department, job, sort_order, episode_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    insert_show_credit: TypedStatement<
        (
            CreditId,
            ShowId,
            PersonId,
            CreditKind,
            Option<String>,
            Option<String>,
            Option<u32>,
            Option<u32>,
        ),
        (),
    >,
    #[sql = "INSERT OR IGNORE INTO show_credit_strings (credit_id, language, kind, text) VALUES (?, ?, ?, ?)"]
    insert_show_credit_string: TypedStatement<(CreditId, api::Locale, api::StringKind, String), ()>,
    #[sql = "DELETE FROM movie_credits WHERE movie_id = ?"]
    clear_movie_credits: TypedStatement<(MovieId,), ()>,
    #[sql = "INSERT INTO movie_credits (id, movie_id, person_id, credit_type, department, job, sort_order, episode_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    insert_movie_credit: TypedStatement<
        (
            CreditId,
            MovieId,
            PersonId,
            CreditKind,
            Option<String>,
            Option<String>,
            Option<u32>,
            Option<u32>,
        ),
        (),
    >,
    #[sql = "INSERT OR IGNORE INTO movie_credit_strings (credit_id, language, kind, text) VALUES (?, ?, ?, ?)"]
    insert_movie_credit_string:
        TypedStatement<(CreditId, api::Locale, api::StringKind, String), ()>,
    #[sql = "DELETE FROM people WHERE id NOT IN (SELECT person_id FROM show_credits UNION SELECT person_id FROM movie_credits)"]
    prune_orphan_people: TypedStatement<(), ()>,

    // seasons
    #[sql = "INSERT INTO seasons (id, show_id, season, air_date)"]
    #[sql = "VALUES (?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(show_id, season) DO UPDATE SET"]
    #[sql = "    air_date  = excluded.air_date"]
    upsert_season: TypedStatement<(SeasonId, ShowId, SeasonNumber, Option<Timestamp>), ()>,
    #[sql = "DELETE FROM seasons WHERE show_id = ?1 AND season = ?2"]
    delete_season: TypedStatement<(ShowId, SeasonNumber), ()>,
    #[sql = "DELETE FROM episodes WHERE show_id = ?1 AND season = ?2"]
    delete_season_episodes: TypedStatement<(ShowId, SeasonNumber), ()>,
    #[sql = "DELETE FROM episodes WHERE show_id = ? AND season = ? AND episode = ?"]
    delete_episode_by_place: TypedStatement<(ShowId, SeasonNumber, u32), ()>,

    // episodes
    #[sql = "INSERT INTO episodes (id, show_id, season, episode, absolute_number, aired)"]
    #[sql = "VALUES (?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(show_id, season, episode) DO UPDATE SET"]
    #[sql = "    absolute_number = excluded.absolute_number,"]
    #[sql = "    aired           = excluded.aired"]
    upsert_episode: TypedStatement<
        (
            EpisodeId,
            ShowId,
            SeasonNumber,
            u32,
            Option<u32>,
            Option<Timestamp>,
        ),
        (),
    >,
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
    #[sql = "DELETE FROM episode_releases"]
    #[sql = "WHERE episode_id = ? AND source = ? AND country = ? AND network = ?"]
    delete_episode_release: TypedStatement<(EpisodeId, RemoteSource, Country, String), ()>,
    #[sql = "UPDATE episodes SET last_synced_at = ? WHERE id = ?"]
    set_episode_synced_at: TypedStatement<(Timestamp, EpisodeId), ()>,

    // per-episode conditional-request state
    #[sql = "INSERT INTO episode_cache (episode_id, source, cache) VALUES (?, ?, ?)"]
    #[sql = "ON CONFLICT(episode_id, source) DO UPDATE SET cache = excluded.cache"]
    set_episode_cache: TypedStatement<(EpisodeId, RemoteSource, String), ()>,
    #[sql = "DELETE FROM episode_cache WHERE episode_id = ? AND source = ?"]
    delete_episode_cache: TypedStatement<(EpisodeId, RemoteSource), ()>,
    #[sql = "DELETE FROM episode_cache"]
    #[sql = "WHERE source = ? AND episode_id IN (SELECT id FROM episodes WHERE show_id = ?)"]
    delete_episode_cache_for_show_source: TypedStatement<(RemoteSource, ShowId), ()>,

    // movies
    #[sql = "INSERT INTO movies (id, release_date)"]
    #[sql = "VALUES (?, ?)"]
    insert_movie: TypedStatement<(MovieId, Option<Timestamp>), ()>,
    #[sql = "INSERT OR IGNORE INTO user_tracked_movies (user_id, movie_id) VALUES (?, ?)"]
    track_movie: TypedStatement<(UserId, MovieId), ()>,
    #[sql = "DELETE FROM user_tracked_movies WHERE user_id = ? AND movie_id = ?"]
    untrack_movie: TypedStatement<(UserId, MovieId), ()>,
    #[sql = "UPDATE movies SET auto_sync = ? WHERE id = ?"]
    set_movie_auto_sync: TypedStatement<(bool, MovieId), ()>,
    #[sql = "UPDATE movies SET release_date = ? WHERE id = ?"]
    set_movie_release_date: TypedStatement<(Option<Timestamp>, MovieId), ()>,
    #[sql = "DELETE FROM movies WHERE id = ?"]
    delete_movie: TypedStatement<(MovieId,), ()>,
    #[sql = "INSERT INTO movie_remotes (id, slug, movie_id, source, value, enabled, priority, sync_kinds) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(movie_id, source, value) DO UPDATE SET slug = COALESCE(excluded.slug, slug)"]
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
    #[sql = "UPDATE movie_remotes SET cache = ? WHERE id = ?"]
    set_movie_remote_cache: TypedStatement<(Option<String>, RemoteId), ()>,
    #[sql = "UPDATE movies SET release_filters = ? WHERE id = ?"]
    set_movie_release_filters: TypedStatement<(Option<String>, MovieId), ()>,

    // per-language translated strings (populated alongside the direct columns
    // during sync; the owner's set is cleared and re-inserted each time)
    #[sql = "UPDATE shows SET default_language = ? WHERE id = ?"]
    set_show_default_language: TypedStatement<(api::Locale, ShowId), ()>,
    #[sql = "UPDATE movies SET default_language = ? WHERE id = ?"]
    set_movie_default_language: TypedStatement<(api::Locale, MovieId), ()>,
    // Seed a credit-created person's display language without clobbering a value
    // already set by its own sync (0 is the unset default; see `seed_person_default_language`).
    #[sql = "UPDATE people SET default_language = ? WHERE id = ? AND default_language = 0"]
    seed_person_default_language: TypedStatement<(api::Locale, PersonId), ()>,
    #[sql = "DELETE FROM show_strings WHERE show_id = ?"]
    clear_show_strings: TypedStatement<(ShowId,), ()>,
    // `OR IGNORE`: the displayed (base) locale and the configured sync languages
    // can overlap, producing duplicate (entity, language, kind) rows in one batch;
    // the first write wins (the text is identical for the same source + locale).
    #[sql = "INSERT OR IGNORE INTO show_strings (show_id, language, kind, text) VALUES (?, ?, ?, ?)"]
    insert_show_string: TypedStatement<(ShowId, api::Locale, api::StringKind, String), ()>,
    #[sql = "DELETE FROM movie_strings WHERE movie_id = ?"]
    clear_movie_strings: TypedStatement<(MovieId,), ()>,
    #[sql = "INSERT OR IGNORE INTO movie_strings (movie_id, language, kind, text) VALUES (?, ?, ?, ?)"]
    insert_movie_string: TypedStatement<(MovieId, api::Locale, api::StringKind, String), ()>,
    #[sql = "DELETE FROM episode_strings WHERE episode_id = ?"]
    clear_episode_strings: TypedStatement<(EpisodeId,), ()>,
    #[sql = "INSERT OR IGNORE INTO episode_strings (episode_id, language, kind, text) VALUES (?, ?, ?, ?)"]
    insert_episode_string: TypedStatement<(EpisodeId, api::Locale, api::StringKind, String), ()>,
    #[sql = "DELETE FROM season_strings WHERE season_id = ?"]
    clear_season_strings: TypedStatement<(SeasonId,), ()>,
    #[sql = "INSERT OR IGNORE INTO season_strings (season_id, language, kind, text) VALUES (?, ?, ?, ?)"]
    insert_season_string: TypedStatement<(SeasonId, api::Locale, api::StringKind, String), ()>,

    // watched
    #[sql = "INSERT OR IGNORE INTO watched_episodes (id, user_id, timestamp, show_id, season, episode)"]
    #[sql = "VALUES (?, ?, ?, ?, ?, ?)"]
    insert_watched_episode:
        TypedStatement<(WatchedId, UserId, Timestamp, ShowId, SeasonNumber, u32), ()>,
    #[sql = "INSERT OR IGNORE INTO watched_movies (id, user_id, timestamp, movie_id)"]
    #[sql = "VALUES (?, ?, ?, ?)"]
    insert_watched_movie: TypedStatement<(WatchedId, UserId, Timestamp, MovieId), ()>,

    // backup import; RETURNING yields a row only when the insert happened
    #[sql = "INSERT OR IGNORE INTO shows (id) VALUES (?)"]
    ensure_show: TypedStatement<(ShowId,), ()>,
    #[sql = "INSERT OR IGNORE INTO movies (id) VALUES (?)"]
    ensure_movie: TypedStatement<(MovieId,), ()>,
    #[sql = "INSERT OR IGNORE INTO user_tracked_shows (user_id, show_id) VALUES (?, ?) RETURNING 1"]
    import_tracked_show: TypedStatement<(UserId, ShowId), (i64,)>,
    #[sql = "INSERT OR IGNORE INTO user_tracked_movies (user_id, movie_id) VALUES (?, ?) RETURNING 1"]
    import_tracked_movie: TypedStatement<(UserId, MovieId), (i64,)>,
    #[sql = "INSERT OR IGNORE INTO user_config (user_id, key, value) VALUES (?, ?, ?) RETURNING 1"]
    import_user_config: TypedStatement<(UserId, PreferenceKey, String), (i64,)>,
    #[sql = "INSERT OR IGNORE INTO user_show_config (user_id, show_id, key, value) VALUES (?, ?, ?, ?) RETURNING 1"]
    import_user_show_config: TypedStatement<(UserId, ShowId, PreferenceKey, String), (i64,)>,
    #[sql = "INSERT OR IGNORE INTO user_movie_config (user_id, movie_id, key, value) VALUES (?, ?, ?, ?) RETURNING 1"]
    import_user_movie_config: TypedStatement<(UserId, MovieId, PreferenceKey, String), (i64,)>,
    #[sql = "DELETE FROM watched_episodes WHERE user_id = ? AND id = ?"]
    delete_watched_episode: TypedStatement<(UserId, WatchedId), ()>,
    #[sql = "DELETE FROM watched_movies WHERE user_id = ? AND id = ?"]
    delete_watched_movie: TypedStatement<(UserId, WatchedId), ()>,
    #[sql = "UPDATE watched_episodes SET season = ?, episode = ? WHERE user_id = ? AND id = ?"]
    move_watched_episode: TypedStatement<(SeasonNumber, u32, UserId, WatchedId), ()>,

    // pending table management
    #[sql = "INSERT INTO pending (id, user_id, timestamp, show_id, episode_id) VALUES (?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(user_id, show_id) WHERE show_id IS NOT NULL"]
    #[sql = "    DO UPDATE SET episode_id = excluded.episode_id, timestamp = excluded.timestamp"]
    upsert_pending_episode: TypedStatement<(PendingId, UserId, Timestamp, ShowId, EpisodeId), ()>,
    #[sql = "INSERT INTO pending (id, user_id, timestamp, movie_id) VALUES (?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(user_id, movie_id) WHERE movie_id IS NOT NULL"]
    #[sql = "    DO UPDATE SET timestamp = excluded.timestamp"]
    upsert_pending_movie: TypedStatement<(PendingId, UserId, Timestamp, MovieId), ()>,
    #[sql = "UPDATE pending SET timestamp = ? WHERE user_id = ? AND show_id = ?"]
    update_pending_episode_timestamp: TypedStatement<(Timestamp, UserId, ShowId), ()>,
    #[sql = "DELETE FROM pending WHERE user_id = ? AND show_id = ?"]
    delete_pending_episode: TypedStatement<(UserId, ShowId), ()>,
    #[sql = "DELETE FROM pending WHERE user_id = ? AND movie_id = ?"]
    delete_pending_movie: TypedStatement<(UserId, MovieId), ()>,

    // preferences
    #[sql = "DELETE FROM user_config WHERE user_id = ?"]
    clear_user_config: TypedStatement<(UserId,), ()>,
    #[sql = "INSERT INTO user_config (user_id, key, value) VALUES (?, ?, ?)"]
    insert_user_config: TypedStatement<(UserId, PreferenceKey, String), ()>,
    #[sql = "INSERT INTO user_show_config (user_id, show_id, key, value) VALUES (?, ?, ?, ?)"]
    #[sql = "ON CONFLICT (user_id, show_id, key) DO UPDATE SET value = excluded.value"]
    set_user_show_config: TypedStatement<(UserId, ShowId, PreferenceKey, String), ()>,
    #[sql = "DELETE FROM user_show_config WHERE user_id = ? AND show_id = ? AND key = ?"]
    delete_user_show_config: TypedStatement<(UserId, ShowId, PreferenceKey), ()>,
    #[sql = "INSERT INTO user_movie_config (user_id, movie_id, key, value) VALUES (?, ?, ?, ?)"]
    #[sql = "ON CONFLICT (user_id, movie_id, key) DO UPDATE SET value = excluded.value"]
    set_user_movie_config: TypedStatement<(UserId, MovieId, PreferenceKey, String), ()>,
    #[sql = "DELETE FROM user_movie_config WHERE user_id = ? AND movie_id = ? AND key = ?"]
    delete_user_movie_config: TypedStatement<(UserId, MovieId, PreferenceKey), ()>,

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
    #[sql = "DELETE FROM movie_releases"]
    #[sql = "WHERE movie_id = ? AND source = ? AND country = ? AND release_type = ?"]
    delete_movie_release: TypedStatement<(MovieId, RemoteSource, Country, ReleaseType), ()>,
    #[sql = "INSERT INTO movie_releases (movie_id, source, country, release_type, timestamp)"]
    #[sql = "VALUES (?, ?, ?, ?, ?)"]
    #[sql = "ON CONFLICT(movie_id, source, country, release_type)"]
    #[sql = "    DO UPDATE SET timestamp = excluded.timestamp"]
    upsert_movie_release:
        TypedStatement<(MovieId, RemoteSource, Country, ReleaseType, Timestamp), ()>,

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
    /// A show's remotes, in priority order.
    fn show_remotes(&mut self, show_id: ShowId) -> Result<Vec<api::RemoteEntry>> {
        let mut out = Vec::new();
        let mut stmt = self.list_show_remotes.bind((show_id,))?;

        while let Some(r) = stmt.next()? {
            out.push(remote_entry(r));
        }

        stmt.reset()?;
        Ok(out)
    }

    /// A movie's remotes, in priority order.
    fn movie_remotes(&mut self, movie_id: MovieId) -> Result<Vec<api::RemoteEntry>> {
        let mut out = Vec::new();
        let mut stmt = self.list_movie_remotes.bind((movie_id,))?;

        while let Some(r) = stmt.next()? {
            out.push(remote_entry(r));
        }

        stmt.reset()?;
        Ok(out)
    }

    /// Map of `(season, number)` to the existing episode id for a show, so a
    /// re-sync can reuse stable ids rather than allocating new ones.
    pub(crate) fn episode_ids(
        &mut self,
        show_id: ShowId,
    ) -> Result<HashMap<(SeasonNumber, u32), EpisodeId>> {
        let mut out = HashMap::new();

        let mut stmt = self.list_episode_ids_for_show.bind((show_id,))?;

        while let Some(r) = stmt.next()? {
            out.insert((r.season, r.number), r.id);
        }

        stmt.reset()?;
        Ok(out)
    }

    pub(crate) fn load_config(&mut self) -> Result<Config> {
        let default = Config::default();

        let tvdb_api_key = self
            .get_config("tvdb_api_key")?
            .unwrap_or_default()
            .to_owned();

        let tvdb_pin = self.get_config("tvdb_pin")?;

        let tmdb_api_key = self
            .get_config("tmdb_api_key")?
            .unwrap_or_default()
            .to_owned();

        let auto_sync_enabled = self
            .get_config("auto_sync_enabled")?
            .map(|v| v == "true")
            .unwrap_or(default.auto_sync_enabled);

        let auto_sync_interval_hours = self
            .get_config("auto_sync_interval_hours")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(default.auto_sync_interval_hours);

        let page_title = self
            .get_config("page_title")?
            .unwrap_or_default()
            .to_owned();

        let release_filters = self
            .get_config("release_filters")?
            .as_deref()
            .and_then(config::decode_filter_rules)
            .unwrap_or(default.release_filters);

        let air_date_filters = self
            .get_config("air_date_filters")?
            .as_deref()
            .and_then(config::decode_filter_rules)
            .unwrap_or(default.air_date_filters);

        let sync_kinds = self
            .get_config("sync_kinds")?
            .as_deref()
            .and_then(config::decode_sync_kinds)
            .unwrap_or(default.sync_kinds);

        let sync_languages = self
            .get_config("sync_languages")?
            .as_deref()
            .and_then(config::decode_sync_languages)
            .unwrap_or(default.sync_languages);

        let cloudflare_access = api::CloudflareAccess {
            enabled: self
                .get_config("cloudflare_access_enabled")?
                .is_some_and(|v| v == "true"),
            team_domain: self
                .get_config("cloudflare_team_domain")?
                .unwrap_or_default(),
            audience: self.get_config("cloudflare_audience")?.unwrap_or_default(),
            trust_email_header: self
                .get_config("cloudflare_trust_email_header")?
                .is_some_and(|v| v == "true"),
            verify_jwt: self
                .get_config("cloudflare_verify_jwt")?
                .is_none_or(|v| v == "true"),
        };

        Ok(Config {
            tvdb_api_key,
            tvdb_pin,
            tmdb_api_key,
            auto_sync_enabled,
            auto_sync_interval_hours,
            page_title,
            release_filters,
            air_date_filters,
            sync_kinds,
            sync_languages,
            cloudflare_access,
        })
    }

    fn get_config(&mut self, key: &str) -> Result<Option<String>> {
        Ok(self.get_config.bind((key,))?.first()?)
    }

    fn trackers_of_show(&mut self, show_id: ShowId) -> Result<Vec<UserId>> {
        let mut out = Vec::new();
        let mut stmt = self.show_trackers.bind((show_id,))?;

        while let Some(user) = stmt.next()? {
            out.push(user);
        }

        stmt.reset()?;
        Ok(out)
    }

    fn trackers_of_movie(&mut self, movie_id: MovieId) -> Result<Vec<UserId>> {
        let mut out = Vec::new();
        let mut stmt = self.movie_trackers.bind((movie_id,))?;

        while let Some(user) = stmt.next()? {
            out.push(user);
        }

        stmt.reset()?;
        Ok(out)
    }
}

impl InnerWrite {
    /// Run `f` inside `BEGIN IMMEDIATE` / `COMMIT`, rolling back when it fails
    /// or panics so the connection is never left inside a transaction.
    fn transaction<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.begin_immediate.execute(())?;

        let result = panic::catch_unwind(AssertUnwindSafe(|| f(self)));

        let result = match result {
            Ok(Ok(value)) => match self.commit.execute(()) {
                Ok(()) => return Ok(value),
                Err(e) => Err(e.into()),
            },
            Ok(Err(e)) => Err(e),
            Err(payload) => {
                _ = self.rollback.execute(());
                panic::resume_unwind(payload);
            }
        };

        // Fails only when SQLite already rolled the transaction back itself.
        _ = self.rollback.execute(());
        result
    }

    /// Delete the show's seasons not in `kept`, with their episodes.
    pub(crate) fn prune_seasons(
        &mut self,
        show_id: ShowId,
        kept: &HashSet<SeasonNumber>,
    ) -> Result<Vec<SeasonNumber>> {
        let mut removed = Vec::new();
        let mut stmt = self.list_seasons.bind((None::<UserId>, show_id))?;

        while let Some(r) = stmt.next()? {
            if !kept.contains(&r.season) {
                removed.push(r.season);
            }
        }

        stmt.reset()?;

        for &n in &removed {
            self.delete_season_episodes.execute((show_id, n))?;
            self.delete_season.execute((show_id, n))?;
        }

        Ok(removed)
    }

    pub(crate) fn update_show(&mut self, id: ShowId, first_air: Option<Timestamp>) -> Result<()> {
        self.update_show.execute((first_air.as_ref(), id))?;
        Ok(())
    }

    pub(crate) fn set_show_default_language(
        &mut self,
        show_id: ShowId,
        language: api::Locale,
    ) -> Result<()> {
        self.set_show_default_language
            .execute((language, show_id))?;
        Ok(())
    }

    /// Replace the owner's translated strings with `strings` (clear then insert),
    /// so languages no longer produced by the sync don't linger.
    pub(crate) fn replace_show_strings(
        &mut self,
        show_id: ShowId,
        strings: Vec<(api::Locale, api::StringKind, String)>,
    ) -> Result<()> {
        self.clear_show_strings.execute((show_id,))?;
        for (language, kind, text) in strings {
            self.insert_show_string
                .execute((show_id, language, kind, text))?;
        }
        Ok(())
    }

    pub(crate) fn clear_show_images(&mut self, show_id: ShowId) -> Result<()> {
        self.delete_show_images.execute((show_id,))?;
        Ok(())
    }

    pub(crate) fn upsert_show_image(
        &mut self,
        id: ImageId,
        show_id: ShowId,
        kind: ImageKind,
        rank: u32,
        image: &Image,
        score: Option<f64>,
    ) -> Result<()> {
        self.insert_show_image.execute((
            id,
            show_id,
            kind,
            image.key().source(),
            image.key().path(),
            image.width(),
            image.height(),
            rank,
            score,
        ))?;

        Ok(())
    }

    pub(crate) fn set_show_image_selection(
        &mut self,
        show_id: ShowId,
        kind: ImageKind,
        image_id: ImageId,
        user_selected: bool,
    ) -> Result<()> {
        self.set_show_image_selection
            .execute((show_id, kind, image_id, user_selected))?;
        Ok(())
    }

    pub(crate) fn clear_episode_images(&mut self, show_id: ShowId) -> Result<()> {
        self.delete_episode_images_for_show.execute((show_id,))?;
        Ok(())
    }

    pub(crate) fn upsert_episode(
        &mut self,
        id: EpisodeId,
        show_id: ShowId,
        season: SeasonNumber,
        number: u32,
        absolute_number: Option<u32>,
        aired: Option<Timestamp>,
    ) -> Result<()> {
        self.upsert_episode.execute((
            id,
            show_id,
            season,
            number,
            absolute_number,
            aired.as_ref(),
        ))?;

        Ok(())
    }

    pub(crate) fn replace_episode_strings(
        &mut self,
        episode_id: EpisodeId,
        strings: Vec<(api::Locale, api::StringKind, String)>,
    ) -> Result<()> {
        self.clear_episode_strings.execute((episode_id,))?;
        for (language, kind, text) in strings {
            self.insert_episode_string
                .execute((episode_id, language, kind, text))?;
        }
        Ok(())
    }

    pub(crate) fn upsert_episode_image(
        &mut self,
        id: ImageId,
        episode_id: EpisodeId,
        kind: ImageKind,
        image: &Image,
    ) -> Result<()> {
        self.insert_episode_image.execute((
            id,
            episode_id,
            kind,
            image.key().source(),
            image.key().path(),
            image.width(),
            image.height(),
        ))?;
        Ok(())
    }

    pub(crate) fn set_episode_image_selection(
        &mut self,
        episode_id: EpisodeId,
        kind: ImageKind,
        image_id: ImageId,
    ) -> Result<()> {
        self.set_episode_image_selection
            .execute((episode_id, kind, image_id))?;
        Ok(())
    }

    pub(crate) fn upsert_season(
        &mut self,
        show_id: ShowId,
        number: SeasonNumber,
        air_date: Option<Timestamp>,
    ) -> Result<SeasonId> {
        self.upsert_season
            .execute((SeasonId::random(), show_id, number, air_date.as_ref()))?;
        let id = self
            .season_id_for
            .bind((show_id, number))?
            .first()?
            .context("Season missing after upsert")?;
        Ok(id)
    }

    pub(crate) fn replace_season_strings(
        &mut self,
        season_id: SeasonId,
        strings: Vec<(api::Locale, api::StringKind, String)>,
    ) -> Result<()> {
        self.clear_season_strings.execute((season_id,))?;

        for (language, kind, text) in strings {
            self.insert_season_string
                .execute((season_id, language, kind, text))?;
        }

        Ok(())
    }

    pub(crate) fn clear_season_images(&mut self, season_id: SeasonId) -> Result<()> {
        self.delete_season_images.execute((season_id,))?;
        Ok(())
    }

    pub(crate) fn upsert_season_image(
        &mut self,
        id: ImageId,
        season_id: SeasonId,
        kind: ImageKind,
        image: &api::Image,
    ) -> Result<()> {
        self.insert_season_image.execute((
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
    }

    pub(crate) fn set_season_image_selection(
        &mut self,
        season_id: SeasonId,
        kind: ImageKind,
        image_id: ImageId,
    ) -> Result<()> {
        self.set_season_image_selection
            .execute((season_id, kind, image_id))?;
        Ok(())
    }

    pub(crate) fn prune_season_episodes(
        &mut self,
        show_id: ShowId,
        season: SeasonNumber,
        kept: &HashSet<u32>,
    ) -> Result<()> {
        let mut to_delete = Vec::new();

        let mut stmt = self.episode_numbers_for_season.bind((show_id, season))?;

        while let Some(number) = stmt.next()? {
            if !kept.contains(&number) {
                to_delete.push(number);
            }
        }

        stmt.reset()?;

        for number in to_delete {
            self.delete_episode_by_place
                .execute((show_id, season, number))?;
        }

        Ok(())
    }

    pub(crate) fn upsert_episode_release(
        &mut self,
        episode_id: EpisodeId,
        source: RemoteSource,
        country: Country,
        network: &str,
        timestamp: Timestamp,
    ) -> Result<()> {
        self.upsert_episode_release
            .execute((episode_id, source, country, network, timestamp))?;
        Ok(())
    }

    pub(crate) fn clear_show_credits(&mut self, show_id: ShowId) -> Result<()> {
        self.clear_show_credits.execute((show_id,))?;
        Ok(())
    }

    /// Find-or-create the bare person for a `(source, remote_id)`, returning the
    /// stable [`PersonId`] and its `last_synced_at` (`None` when never synced, so
    /// the caller can seed placeholder data). The person's own data is filled in
    /// by [`sync_person`](crate::sync::sync_person).
    pub(crate) fn upsert_person(
        &mut self,
        source: RemoteSource,
        remote_id: u32,
    ) -> Result<(PersonId, Option<Timestamp>)> {
        let value = RemoteValue::Int(remote_id);
        let existing = self
            .person_by_remote
            .bind((source, value.clone()))?
            .first()?;

        if let Some((id, last_synced)) = existing {
            Ok((id, last_synced))
        } else {
            let id = PersonId::random();
            self.insert_person.execute((id,))?;

            // Seed the person's identity as its primary remote, mirroring how a
            // show/movie's base remote is created.
            let remote_id = RemoteId::random();
            self.insert_person_remote.execute((
                remote_id,
                None::<String>,
                id,
                source,
                value,
                true,
                0i32,
                None::<api::SyncKindSet>,
            ))?;
            self.set_person_primary_remote.execute((remote_id, id))?;
            Ok((id, None))
        }
    }

    /// Seed a placeholder person string (name) without overwriting an existing
    /// one - used from the credit sync so the cast grid is populated before the
    /// person's own sync runs.
    pub(crate) fn seed_person_string(
        &mut self,
        person_id: PersonId,
        language: api::Locale,
        kind: api::StringKind,
        text: &str,
    ) -> Result<()> {
        self.insert_person_string
            .execute((person_id, language, kind, text))?;
        Ok(())
    }

    /// Seed a credit-created person's display language, used so the seeded name
    /// resolves on the person page/list before the person's own sync sets the
    /// authoritative value. A no-op once a real sync has set the language.
    pub(crate) fn seed_person_default_language(
        &mut self,
        person_id: PersonId,
        language: api::Locale,
    ) -> Result<()> {
        self.seed_person_default_language
            .execute((language, person_id))?;
        Ok(())
    }

    /// Seed a placeholder profile image (rank 0), ignored if the person already
    /// has that image. Cleared and replaced by the person's own sync.
    pub(crate) fn seed_person_image(
        &mut self,
        person_id: PersonId,
        kind: ImageKind,
        image: &Image,
    ) -> Result<()> {
        self.insert_person_image.execute((
            ImageId::random(),
            person_id,
            kind,
            image.key().source(),
            image.key().path(),
            image.width(),
            image.height(),
            0,
            None::<f64>,
        ))?;
        Ok(())
    }

    pub(crate) fn insert_show_credit(
        &mut self,
        credit_id: CreditId,
        show_id: ShowId,
        person_id: PersonId,
        kind: CreditKind,
        department: Option<&str>,
        job: Option<&str>,
        order: Option<u32>,
        episode_count: Option<u32>,
    ) -> Result<()> {
        self.insert_show_credit.execute((
            credit_id,
            show_id,
            person_id,
            kind,
            department,
            job,
            order,
            episode_count,
        ))?;
        Ok(())
    }

    pub(crate) fn insert_show_credit_string(
        &mut self,
        credit_id: CreditId,
        language: api::Locale,
        kind: api::StringKind,
        text: &str,
    ) -> Result<()> {
        self.insert_show_credit_string
            .execute((credit_id, language, kind, text))?;
        Ok(())
    }

    /// Delete people no longer referenced by any credit (their profile images
    /// cascade). Run after rewriting an owner's credits.
    pub(crate) fn prune_orphan_people(&mut self) -> Result<()> {
        self.prune_orphan_people.execute(())?;
        Ok(())
    }

    /// Delete a show's images from a single source, leaving other sources'
    /// images (and the show's overall selection rows) intact.
    pub(crate) fn delete_show_images_for_source(
        &mut self,
        show_id: ShowId,
        source: ImageSource,
    ) -> Result<()> {
        self.delete_show_images_for_source
            .execute((show_id, source))?;
        Ok(())
    }

    /// Drop every image attached to a single episode. The show-scoped
    /// [`Self::clear_episode_images`] would wipe every *other* episode's screenshot
    /// too, which a single-episode sync must not do.
    pub(crate) fn clear_images_for_episode(&mut self, id: EpisodeId) -> Result<()> {
        self.delete_images_for_episode.execute((id,))?;
        Ok(())
    }

    pub(crate) fn set_movie_default_language(
        &mut self,
        movie_id: MovieId,
        language: api::Locale,
    ) -> Result<()> {
        self.set_movie_default_language
            .execute((language, movie_id))?;
        Ok(())
    }

    pub(crate) fn replace_movie_strings(
        &mut self,
        movie_id: MovieId,
        strings: Vec<(api::Locale, api::StringKind, String)>,
    ) -> Result<()> {
        self.clear_movie_strings.execute((movie_id,))?;
        for (language, kind, text) in strings {
            self.insert_movie_string
                .execute((movie_id, language, kind, text))?;
        }
        Ok(())
    }

    pub(crate) fn clear_movie_images(&mut self, movie_id: MovieId) -> Result<()> {
        self.delete_movie_images.execute((movie_id,))?;
        Ok(())
    }

    pub(crate) fn upsert_movie_image(
        &mut self,
        id: ImageId,
        movie_id: MovieId,
        kind: ImageKind,
        rank: u32,
        image: &Image,
        score: Option<f64>,
    ) -> Result<()> {
        self.insert_movie_image.execute((
            id,
            movie_id,
            kind,
            image.key().source(),
            image.key().path(),
            image.width(),
            image.height(),
            rank,
            score,
        ))?;
        Ok(())
    }

    pub(crate) fn set_movie_image_selection(
        &mut self,
        movie_id: MovieId,
        kind: ImageKind,
        image_id: ImageId,
        user_selected: bool,
    ) -> Result<()> {
        self.set_movie_image_selection
            .execute((movie_id, kind, image_id, user_selected))?;
        Ok(())
    }

    pub(crate) fn clear_movie_credits(&mut self, movie_id: MovieId) -> Result<()> {
        self.clear_movie_credits.execute((movie_id,))?;
        Ok(())
    }

    pub(crate) fn insert_movie_credit(
        &mut self,
        credit_id: CreditId,
        movie_id: MovieId,
        person_id: PersonId,
        kind: CreditKind,
        department: Option<&str>,
        job: Option<&str>,
        order: Option<u32>,
        episode_count: Option<u32>,
    ) -> Result<()> {
        self.insert_movie_credit.execute((
            credit_id,
            movie_id,
            person_id,
            kind,
            department,
            job,
            order,
            episode_count,
        ))?;
        Ok(())
    }

    pub(crate) fn insert_movie_credit_string(
        &mut self,
        credit_id: CreditId,
        language: api::Locale,
        kind: api::StringKind,
        text: &str,
    ) -> Result<()> {
        self.insert_movie_credit_string
            .execute((credit_id, language, kind, text))?;
        Ok(())
    }

    pub(crate) fn upsert_movie_release(
        &mut self,
        movie_id: MovieId,
        source: RemoteSource,
        country: Country,
        release_type: ReleaseType,
        timestamp: &Timestamp,
    ) -> Result<()> {
        self.upsert_movie_release
            .execute((movie_id, source, country, release_type, timestamp))?;

        Ok(())
    }
    /// The next free rank per kind for a show's images (current max + 1), so
    /// freshly merged images append after the ones already stored.
    pub(crate) fn next_show_image_ranks(
        &mut self,
        show_id: ShowId,
    ) -> Result<HashMap<ImageKind, u32>> {
        let mut out = HashMap::new();
        let mut stmt = self.show_image_max_ranks.bind((show_id,))?;

        while let Some(r) = stmt.next()? {
            out.insert(r.kind, r.rank + 1);
        }

        stmt.reset()?;
        Ok(out)
    }

    /// The kinds a show currently has a selected image for.
    pub(crate) fn show_selected_image_kinds(
        &mut self,
        show_id: ShowId,
    ) -> Result<HashSet<ImageKind>> {
        let mut out = HashSet::new();
        let mut stmt = self.show_selected_image_kinds.bind((show_id,))?;

        while let Some(kind) = stmt.next()? {
            out.insert(kind);
        }

        stmt.reset()?;
        Ok(out)
    }

    /// Same as [`user_selected_show_image_keys`], for a movie.
    pub(crate) fn user_selected_movie_image_keys(
        &mut self,
        movie_id: MovieId,
    ) -> Result<HashMap<ImageKind, ImageKey>> {
        let mut out = HashMap::new();
        let mut stmt = self.user_selected_movie_images.bind((movie_id,))?;

        while let Some(r) = stmt.next()? {
            out.insert(r.kind, ImageKey::new(r.source, &r.path));
        }

        stmt.reset()?;
        Ok(out)
    }

    /// Kind + key of the show's user-chosen selections, so a sync that clears
    /// and rebuilds image rows can re-attach them.
    pub(crate) fn user_selected_show_image_keys(
        &mut self,
        show_id: ShowId,
    ) -> Result<HashMap<ImageKind, ImageKey>> {
        let mut out = HashMap::new();
        let mut stmt = self.user_selected_show_images.bind((show_id,))?;

        while let Some(r) = stmt.next()? {
            out.insert(r.kind, ImageKey::new(r.source, &r.path));
        }

        stmt.reset()?;
        Ok(out)
    }

    pub(crate) fn add_show_remote(
        &mut self,
        show_id: ShowId,
        slug: Option<&str>,
        remote: &Remote,
    ) -> Result<()> {
        let config = self.load_config()?;
        let priority = default_remote_priority(*remote.source(), &config);

        self.insert_show_remote.execute((
            RemoteId::random(),
            slug,
            show_id,
            remote.source(),
            remote.value(),
            true,
            priority,
            // NULL = inherit the global per-source sync-kinds default.
            None::<api::SyncKindSet>,
        ))?;
        Ok(())
    }

    pub(crate) fn add_movie_remote(
        &mut self,
        movie_id: MovieId,
        slug: Option<&str>,
        remote: &Remote,
    ) -> Result<()> {
        let config = self.load_config()?;
        let priority = default_remote_priority(*remote.source(), &config);

        self.insert_movie_remote.execute((
            RemoteId::random(),
            slug,
            movie_id,
            remote.source(),
            remote.value(),
            true,
            priority,
            // NULL = inherit the global per-source sync-kinds default.
            None::<api::SyncKindSet>,
        ))?;
        Ok(())
    }

    /// Drop stored releases that are no longer current. A release is retained only
    /// while its source is an *eligible* air-date source (an enabled remote
    /// configured to contribute air dates): such a source that ran this sync keeps
    /// the releases it just reported (`kept`) and loses any stale ones, while one
    /// that didn't run keeps all of its releases so a transient fetch failure
    /// doesn't wipe them. Releases from any other source — disabled, or `Unknown`
    /// (e.g. the legacy air-date backfill) — can never be contributed again and are
    /// always pruned. `ran` is the set of sources whose air-date layer ran this
    /// sync; `kept` is the `(episode_id, source, country, network)` tuples just
    /// upserted.
    pub(crate) fn prune_episode_releases(
        &mut self,
        show_id: ShowId,
        kept: &HashSet<(EpisodeId, RemoteSource, Country, String)>,
        ran: &HashSet<RemoteSource>,
    ) -> Result<()> {
        let config = self.load_config()?;
        let eligible: HashSet<RemoteSource> =
            api::air_date_sources_by_priority(&self.show_remotes(show_id)?, &config)
                .into_iter()
                .collect();

        let mut to_delete = Vec::new();

        let mut stmt = self.list_episode_releases_for_show.bind((show_id,))?;

        while let Some(r) = stmt.next()? {
            // An eligible source that didn't run this sync keeps its releases (a
            // transient fetch failure); every other source is pruned down to what
            // it just reported, so disabled and `Unknown` sources are dropped.
            if eligible.contains(&r.source) && !ran.contains(&r.source) {
                continue;
            }

            let key = (r.episode_id, r.source, r.country, r.network);

            if !kept.contains(&key) {
                to_delete.push(key);
            }
        }

        stmt.reset()?;

        for (episode_id, source, country, network) in to_delete {
            self.delete_episode_release
                .execute((episode_id, source, country, network))?;
        }

        Ok(())
    }

    /// The single-episode counterpart of [`Self::prune_episode_releases`], with the
    /// same eligibility rule. Scoping matters: the show-wide version walks every
    /// episode of the show, so using it after a single-episode sync would delete
    /// every *other* episode's releases from the sources that just ran.
    pub(crate) fn prune_episode_releases_for_episode(
        &mut self,
        show_id: ShowId,
        episode_id: EpisodeId,
        kept: &HashSet<(RemoteSource, Country, String)>,
        ran: &HashSet<RemoteSource>,
    ) -> Result<()> {
        let config = self.load_config()?;
        let eligible: HashSet<RemoteSource> =
            api::air_date_sources_by_priority(&self.show_remotes(show_id)?, &config)
                .into_iter()
                .collect();

        let mut to_delete = Vec::new();

        let mut stmt = self.list_episode_releases.bind((episode_id,))?;

        while let Some((source, country, network, _)) = stmt.next()? {
            // An eligible source that didn't run keeps its releases (a transient
            // fetch failure); every other source is pruned down to what it just
            // reported.
            if eligible.contains(&source) && !ran.contains(&source) {
                continue;
            }

            let key = (source, country, network);

            if !kept.contains(&key) {
                to_delete.push(key);
            }
        }

        stmt.reset()?;

        for (source, country, network) in to_delete {
            self.delete_episode_release
                .execute((episode_id, source, country, network))?;
        }

        Ok(())
    }

    /// Drop stored releases that a fresh sync no longer reports. `kept` is the set
    /// of `(source, country, release_type)` keys just upserted for the movie; `ran`
    /// is the set of sources whose release layer actually ran this sync. Mirrors
    /// [`Self::prune_episode_releases`]: a release-eligible source that didn't run
    /// keeps its stored releases (a transient fetch failure or a cache hit), so the
    /// caller can prune safely even when only some sources ran.
    pub(crate) fn prune_movie_releases(
        &mut self,
        movie_id: MovieId,
        kept: &HashSet<(RemoteSource, Country, ReleaseType)>,
        ran: &HashSet<RemoteSource>,
    ) -> Result<()> {
        let config = self.load_config()?;
        let eligible: HashSet<RemoteSource> =
            api::air_date_sources_by_priority(&self.movie_remotes(movie_id)?, &config)
                .into_iter()
                .collect();

        let mut to_delete = Vec::new();

        let mut stmt = self.list_movie_releases.bind((movie_id,))?;

        while let Some(r) = stmt.next()? {
            // An eligible source that didn't run this sync keeps its releases;
            // every other source is pruned down to what it just reported.
            if eligible.contains(&r.source) && !ran.contains(&r.source) {
                continue;
            }

            let key = (r.source, r.country, r.release_type);

            if !kept.contains(&key) {
                to_delete.push(key);
            }
        }

        stmt.reset()?;

        for (source, country, release_type) in to_delete {
            self.delete_movie_release
                .execute((movie_id, source, country, release_type))?;
        }

        Ok(())
    }

    /// Store the user's preference for a show; the default removes the row.
    fn set_show_pref<T>(
        &mut self,
        user: UserId,
        show: ShowId,
        key: PreferenceKey,
        value: T,
    ) -> Result<()>
    where
        T: PreferenceValue + Default + PartialEq,
    {
        if value == T::default() {
            self.delete_user_show_config.execute((user, show, key))?;
        } else {
            self.set_user_show_config
                .execute((user, show, key, value.to_json()))?;
        }

        Ok(())
    }

    /// Store the user's preference for a movie; the default removes the row.
    fn set_movie_pref<T>(
        &mut self,
        user: UserId,
        movie: MovieId,
        key: PreferenceKey,
        value: T,
    ) -> Result<()>
    where
        T: PreferenceValue + Default + PartialEq,
    {
        if value == T::default() {
            self.delete_user_movie_config.execute((user, movie, key))?;
        } else {
            self.set_user_movie_config
                .execute((user, movie, key, value.to_json()))?;
        }

        Ok(())
    }

    /// Fill the user's pending slot for a show unless it already has one, in
    /// which case only a changed future air date is carried over.
    fn fill_pending_for_show(
        &mut self,
        user: UserId,
        show_id: ShowId,
        now: Timestamp,
    ) -> Result<()> {
        let already_has = self
            .has_pending_episode_for_show
            .bind((user, show_id))?
            .first()?
            .is_some();

        if already_has {
            let row = self
                .pending_episode_aired_for_show
                .bind((user, show_id))?
                .first()?;

            let maybe_update = row.and_then(|r| {
                let aired = r.aired?;

                if aired > now && aired != r.timestamp {
                    Some(aired)
                } else {
                    None
                }
            });

            if let Some(aired) = maybe_update {
                self.update_pending_episode_timestamp
                    .execute((aired, user, show_id))?;
            }

            return Ok(());
        }

        let include_specials = match self.user_include_specials.bind((user, show_id))?.first()? {
            Some((Pref(show), Pref(global))) => show.unwrap_or(global),
            None => false,
        };

        let Some(row) = self
            .next_pending_episode_for_show
            .bind((user, show_id, include_specials))?
            .first()?
        else {
            return Ok(());
        };

        let ts = row.aired.unwrap_or(now).max(now);

        self.upsert_pending_episode
            .execute((PendingId::random(), user, ts, show_id, row.id))?;

        Ok(())
    }

    fn set_config(&mut self, key: &str, value: impl AsRef<str>) -> Result<()> {
        self.set_config.execute((key, value.as_ref()))?;
        Ok(())
    }

    fn delete_config(&mut self, key: &str) -> Result<()> {
        self.delete_config.execute((key,))?;
        Ok(())
    }
}

impl InnerRead {
    fn export_snapshot(&mut self) -> Result<ExportSnapshot> {
        let users = self.users.list()?;

        let mut show_remotes = Vec::new();
        let mut stmt = self.list_all_show_remotes.query()?;

        while let Some(r) = stmt.next()? {
            show_remotes.push(ExportRemote {
                id: r.id,
                owner: r.show_id,
                source: r.source,
                value: r.value,
                slug: r.slug,
                enabled: r.enabled,
                priority: r.priority,
                sync_kinds: r.sync_kinds,
            });
        }

        stmt.reset()?;

        let mut movie_remotes = Vec::new();
        let mut stmt = self.list_all_movie_remotes.query()?;

        while let Some(r) = stmt.next()? {
            movie_remotes.push(ExportRemote {
                id: r.id,
                owner: r.movie_id,
                source: r.source,
                value: r.value,
                slug: r.slug,
                enabled: r.enabled,
                priority: r.priority,
                sync_kinds: r.sync_kinds,
            });
        }

        stmt.reset()?;

        let user_data = ExportUserData {
            tracked_shows: all_rows(&mut self.list_all_tracked_shows)?,
            tracked_movies: all_rows(&mut self.list_all_tracked_movies)?,
            user_config: all_rows(&mut self.list_all_user_config)?,
            show_config: all_rows(&mut self.list_all_user_show_config)?,
            movie_config: all_rows(&mut self.list_all_user_movie_config)?,
        };

        // Orphaned watched rows (NULL show or movie) can't be attributed and are
        // skipped.
        let mut watched_episodes = Vec::new();
        let mut stmt = self.list_all_watched_episodes.query()?;

        while let Some(r) = stmt.next()? {
            if let Some(show_id) = r.show_id {
                watched_episodes.push((r.id, r.login, r.timestamp, show_id, r.season, r.episode));
            }
        }

        stmt.reset()?;

        let mut watched_movies = Vec::new();
        let mut stmt = self.list_all_watched_movies.query()?;

        while let Some(r) = stmt.next()? {
            if let Some(movie_id) = r.movie_id {
                watched_movies.push((r.id, r.login, r.timestamp, movie_id));
            }
        }

        stmt.reset()?;

        Ok(ExportSnapshot {
            users,
            show_remotes,
            movie_remotes,
            user_data,
            watched_episodes,
            watched_movies,
        })
    }

    /// The user's display locale ([`Locale::DEFAULT`] for nobody or when unset).
    fn user_language(&mut self, user: impl Into<Option<UserId>>) -> Result<api::Locale> {
        let language = self.user_language.bind((user.into(),))?.first()?;
        Ok(language.map(|Pref(l)| l).unwrap_or_default())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum OpenMode {
    /// Full synchronization safe for the server.
    Normal,
    /// No journaling or fsync fast for bulk import; not crash-safe.
    Bulk,
    /// Only reads an existing database: never creates the file, and refuses one
    /// with pending migrations instead of migrating it.
    ReadOnly,
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

        if let OpenMode::ReadOnly = mode {
            let c = OpenOptions::new()
                .extended_result_codes()
                .read_only()
                .no_mutex()
                .open(path.as_os_str())
                .with_context(|| anyhow!("Opening database at {}", path.display()))?;

            ensure_migrated(&c)?;
        } else {
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

        let builder = PoolBuilder::new(options, read_concurrency)
            .with_write_setup(move |c| ensure_mode(c, mode))
            .with_read_setup(move |c| ensure_mode(c, mode));

        Ok(Self {
            inner: Arc::new(builder.open(path)?),
        })
    }

    /// Run `f` on the write connection in one transaction (see
    /// [`InnerWrite::transaction`]).
    pub(crate) async fn transaction<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut InnerWrite) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let mut s = self.inner.clone().exclusive().await?;
        spawn_blocking(move || s.transaction(f)).await?
    }

    #[cfg(test)]
    pub(crate) async fn upsert_episode(
        &self,
        id: EpisodeId,
        show_id: ShowId,
        season: SeasonNumber,
        number: u32,
        absolute_number: Option<u32>,
        aired: Option<Timestamp>,
    ) -> Result<()> {
        self.transaction(move |s| {
            s.upsert_episode(id, show_id, season, number, absolute_number, aired)
        })
        .await
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

        let result = self.transaction(move |s| {
            s.insert_show.execute((id, first_air.as_ref()))?;

            // Store the placeholder title/overview under the default locale so the
            // show has something to show before its first sync; sync replaces these.
            if !title.trim().is_empty() {
                s.insert_show_string.execute((
                    id,
                    api::Locale::DEFAULT,
                    api::StringKind::Title,
                    &title[..],
                ))?;
            }
            if !overview.trim().is_empty() {
                s.insert_show_string.execute((
                    id,
                    api::Locale::DEFAULT,
                    api::StringKind::Overview,
                    &overview[..],
                ))?;
            }
            Ok(())
        });

        result.await
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
        let slug = slug.map(str::to_owned);
        let remote = remote.clone();

        self.transaction(move |s| s.add_show_remote(show_id, slug.as_deref(), &remote))
            .await
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
    pub(crate) async fn shows(&self, user: Option<UserId>) -> Result<Vec<api::Show>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out: Vec<api::Show> = Vec::new();
            let mut id_to_idx: HashMap<ShowId, usize> = HashMap::new();

            let config = s.user_language(user)?;

            let mut stmt = s.list_shows.bind((user,))?;

            while let Some(r) = stmt.next()? {
                id_to_idx.insert(r.id, out.len());
                let strings =
                    api::Translations::new(r.language.0.or(config).or(r.default_language));
                out.push(show_from_row(r, strings));
            }

            stmt.reset()?;

            let mut stmt = s.list_all_show_strings.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&r.show_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.strings.insert(r.kind, r.language, &r.text);
                }
            }

            stmt.reset()?;

            for show in &mut out {
                show.poster = s.image.image_for_show(show.id, ImageKind::Poster)?;
                show.banner = s.image.image_for_show(show.id, ImageKind::Banner)?;
            }

            let mut stmt = s.list_all_show_remotes.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&r.show_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.remotes.push(api::RemoteEntry {
                        id: r.id,
                        slug: r.slug,
                        remote: Remote::new(r.source, r.value),
                        enabled: r.enabled,
                        priority: r.priority,
                        sync_kinds: r.sync_kinds,
                        cache: parse_remote_cache(r.cache),
                    });
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_show_images.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&r.show_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.images.push(show_image_from_row(r));
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_show_image_selections.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&r.show_id)
                    && let Some(o) = out.get_mut(i)
                {
                    apply_image_selection(
                        o,
                        ImageSelectionRow {
                            kind: r.kind,
                            source: r.source,
                            path: r.path,
                            width: r.width,
                            height: r.height,
                            user_selected: r.user_selected,
                        },
                    );
                }
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn show_by_id(
        &self,
        user: Option<UserId>,
        id: ShowId,
    ) -> Result<Option<api::Show>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let Some(r) = s.show_by_id.bind((user, id))?.first()? else {
                return Ok(None);
            };

            let config = s.user_language(user)?;
            let strings = s.translations.show(user, id, config)?;
            let mut show = show_from_row(r, strings);

            show.remotes = s.show_remotes(id)?;

            let mut stmt = s.list_show_images.bind((id,))?;

            while let Some(r) = stmt.next()? {
                show.images.push(image_from_row(r));
            }

            stmt.reset()?;

            let mut stmt = s.list_show_image_selections.bind((id,))?;

            while let Some(r) = stmt.next()? {
                apply_image_selection(&mut show, r);
            }

            stmt.reset()?;

            show.poster = s.image.image_for_show(show.id, ImageKind::Poster)?;
            show.banner = s.image.image_for_show(show.id, ImageKind::Banner)?;
            Ok(Some(show))
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
    pub(crate) async fn set_show_tracked(
        &self,
        user: UserId,
        id: ShowId,
        tracked: bool,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            if tracked {
                s.track_show.execute((user, id))?;
            } else {
                s.untrack_show.execute((user, id))?;
            }

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

    /// Replace a show remote's cached conditional-request state (JSON), or clear
    /// it with `None`.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_remote_cache(
        &self,
        remote_id: RemoteId,
        cache: Option<String>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_show_remote_cache.execute((cache, remote_id))?;
            Ok(())
        });

        result.await?
    }

    /// Set remote priority to match the given order (first = highest priority).
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn reorder_show_remotes(&self, remote_ids: Vec<RemoteId>) -> Result<()> {
        let result = self.transaction(move |s| {
            for (idx, id) in remote_ids.iter().enumerate() {
                s.set_show_remote_priority.execute((idx as i32, *id))?;
            }
            Ok(())
        });

        result.await
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_language(
        &self,
        user: UserId,
        id: ShowId,
        language: api::Locale,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result =
            spawn_blocking(move || s.set_show_pref(user, id, PreferenceKey::Language, language));

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_include_specials(
        &self,
        user: UserId,
        id: ShowId,
        include_specials: IncludeSpecials,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_show_pref(user, id, PreferenceKey::IncludeSpecials, include_specials)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn seasons(
        &self,
        user: Option<UserId>,
        show_id: ShowId,
    ) -> Result<Vec<api::Season>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let config = s.user_language(user)?;

            let (language, default) = s
                .translations
                .show_locales
                .bind((user, show_id))?
                .first()?
                .map(|r| (r.language.0, r.default_language))
                .unwrap_or_default();

            let mut id_to_idx: HashMap<SeasonId, usize> = HashMap::new();

            let mut stmt = s.list_seasons.bind((user, show_id))?;

            while let Some(r) = stmt.next()? {
                id_to_idx.insert(r.id, out.len());
                let strings = api::Translations::new(language.or(config).or(default));
                out.push(season_from_row(r, strings));
            }

            stmt.reset()?;

            let mut stmt = s.list_show_season_strings.bind((show_id,))?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&r.season_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.strings.insert(r.kind, r.language, &r.text);
                }
            }

            stmt.reset()?;

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
        let kept = kept.clone();
        self.transaction(move |s| s.prune_seasons(show_id, &kept))
            .await
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    /// Find the episode whose `aired` timestamp is closest to `timestamp`,
    /// across the whole show. Only considers episodes that have aired and skips
    /// specials.
    pub(crate) async fn find_episode_by_timestamp(
        &self,
        show_id: ShowId,
        timestamp: Timestamp,
    ) -> Result<Option<api::EpisodeMatch>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let row = s
                .find_episode_by_timestamp
                .bind((show_id, timestamp))?
                .first()?;

            Ok(row.map(|r| api::EpisodeMatch {
                season: r.season,
                episode: r.episode,
            }))
        });

        result.await?
    }

    pub(crate) async fn episodes(
        &self,
        user: UserId,
        show_id: ShowId,
        season: SeasonNumber,
    ) -> Result<Vec<api::Episode>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut idx_by_id = HashMap::new();

            let config = s.user_language(user)?;

            let (language, default) = s
                .translations
                .show_locales
                .bind((Some(user), show_id))?
                .first()?
                .map(|r| (r.language.0, r.default_language))
                .unwrap_or_default();

            let mut stmt = s.list_episodes.bind((Some(user), show_id, season))?;

            while let Some(r) = stmt.next()? {
                idx_by_id.insert(r.id, out.len());
                let strings = api::Translations::new(language.or(config).or(default));
                out.push(episode_from_row(r, strings));
            }

            stmt.reset()?;

            let mut stmt = s.list_season_episode_strings.bind((show_id, season))?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = idx_by_id.get(&r.episode_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.strings.insert(r.kind, r.language, &r.text);
                }
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
            Ok(out)
        });

        result.await?
    }

    /// Load a single episode, the way [`Self::episodes`] loads a season's worth.
    pub(crate) async fn episode_by_id(
        &self,
        user: Option<UserId>,
        id: EpisodeId,
    ) -> Result<Option<api::Episode>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let s = &mut *s;

            let Some(r) = s.episode_by_id.bind((user, id))?.first()? else {
                return Ok(None);
            };

            let config = s.user_language(user)?;

            let (language, default) = s
                .translations
                .show_locales
                .bind((user, r.show_id))?
                .first()?
                .map(|r| (r.language.0, r.default_language))
                .unwrap_or_default();

            let strings = s.translations.episode(id, language, default, config)?;
            let mut episode = episode_from_row(r, strings);

            if let Some(i) = s
                .episode_screenshot
                .bind((ImageKind::Screenshot, id))?
                .first()?
            {
                episode.screenshot =
                    Some(Image::new_with_dims(i.source, &i.path, i.width, i.height));
            }

            Ok(Some(episode))
        });

        result.await?
    }

    /// The per-source conditional-request state stored for one episode. Sources with
    /// no validator (or an unparsable one) are simply absent.
    pub(crate) async fn episode_cache(
        &self,
        id: EpisodeId,
    ) -> Result<HashMap<RemoteSource, api::RemoteCache>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = HashMap::new();
            let mut stmt = s.episode_cache.bind((id,))?;

            while let Some((source, cache)) = stmt.next()? {
                if let Some(cache) = parse_remote_cache(Some(cache)) {
                    out.insert(source, cache);
                }
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// Store (or, with `None`, drop) a source's conditional-request state for one
    /// episode.
    pub(crate) async fn set_episode_cache(
        &self,
        id: EpisodeId,
        source: RemoteSource,
        cache: Option<String>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            match cache {
                Some(cache) => s.set_episode_cache.execute((id, source, cache))?,
                None => s.delete_episode_cache.execute((id, source))?,
            };

            Ok(())
        });

        result.await?
    }

    /// Drop every episode-level validator a source holds for one show. Called when
    /// that show's remote cache is purged, so a forced resync really does start from
    /// scratch rather than leaving the per-episode ETags behind.
    pub(crate) async fn clear_episode_cache_for_show_source(
        &self,
        show_id: ShowId,
        source: RemoteSource,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_episode_cache_for_show_source
                .execute((source, show_id))?;
            Ok(())
        });

        result.await?
    }

    pub(crate) async fn set_episode_synced_at(&self, id: EpisodeId, at: Timestamp) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_episode_synced_at.execute((at, id))?;
            Ok(())
        });

        result.await?
    }

    /// Episodes whose air date falls within `window_hours` either side of now and
    /// that haven't been episode-synced in `interval_hours`. Honors the per-show
    /// `auto_sync` flag, and puts never-synced episodes first - mirroring
    /// [`Self::shows_needing_sync`].
    pub(crate) async fn episodes_needing_air_sync(
        &self,
        window_hours: u32,
        interval_hours: u32,
    ) -> Result<Vec<(ShowId, EpisodeId, api::Code)>> {
        let hours = i64::from(window_hours);
        let now = Timestamp::now();
        let start = now.saturating_add(api::Duration::from_hours(-hours));
        let end = now.saturating_add(api::Duration::from_hours(hours));
        let cutoff = cutoff_timestamp(interval_hours);

        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.episodes_needing_air_sync.bind((start, end, cutoff))?;

            while let Some(r) = stmt.next()? {
                out.push((r.show_id, r.id, api::Code::new(r.season, r.episode)));
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn mark_watched_remaining(
        &self,
        user: UserId,
        show_id: ShowId,
        season: SeasonNumber,
        mark_time: MarkTime,
        now: Timestamp,
    ) -> Result<Option<EpisodeId>> {
        let result = self.transaction(move |s| {
            let s = &mut *s;
            let read = &mut s.read;

            let mut stmt = read
                .select_unwatched_by_show_season
                .bind((user, show_id, season))?;

            let mut last = None::<(u32, EpisodeId)>;

            while let Some(r) = stmt.next()? {
                if last.is_none_or(|(number, _)| r.number > number) {
                    last = Some((r.number, r.id));
                }

                let timestamp = read.episodes.episode_mark_time(r.id, mark_time, now)?;

                s.insert_watched_episode.execute((
                    WatchedId::random(),
                    user,
                    timestamp,
                    r.show_id,
                    r.season,
                    r.number,
                ))?;
            }

            stmt.reset()?;
            Ok(last.map(|(_, id)| id))
        });

        result.await
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn episodes_watched(
        &self,
        user: UserId,
        show_id: ShowId,
    ) -> Result<Vec<api::WatchedEpisode>> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_episodes_watched.bind((user, show_id))?;

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
            let stmt = s.episodes.episode_aired_by_id.bind((id,))?;
            Ok(stmt.first()?.flatten())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_show_air_date_filters(
        &self,
        id: ShowId,
        air_date_filters: Option<api::FilterRules>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let text = air_date_filters.as_ref().map(config::encode_filter_rules);
            s.update_show_air_date_filters.execute((text, id))?;
            Ok(())
        });

        result.await?
    }

    /// Recompute each episode's effective `aired` from its stored releases, using
    /// the show's air-date-eligible remote priority and air-date rules (falling
    /// back to `default_filters`). An episode whose releases all come from excluded
    /// sources (or are filtered out) has its date cleared; when no source is
    /// eligible at all (air dates excluded from every remote) every episode's date
    /// is cleared. Episodes with no stored release are left untouched.
    #[tracing::instrument(skip(self, default_filters), ret(level = "trace"))]
    pub(crate) async fn recompute_episode_aired_for_show(
        &self,
        show_id: ShowId,
        default_filters: api::FilterRules,
    ) -> Result<()> {
        let Some(show) = self.show_by_id(None, show_id).await? else {
            return Ok(());
        };

        let config = self.load_config().await?;
        let priority = api::air_date_sources_by_priority(&show.remotes, &config);
        let filters = show.air_date_filters.unwrap_or(default_filters);

        let result = self.transaction(move |s| {
            // No eligible source: drop every air date so excluded dates are no
            // longer shown (stale releases may remain stored, ready to be restored
            // if a source is re-enabled and the show recomputed).
            if priority.is_empty() {
                s.clear_episode_aired_for_show.execute((show_id,))?;
                return Ok(());
            }

            let mut by_episode: HashMap<EpisodeId, Vec<api::EpisodeRelease>> = HashMap::new();

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

            stmt.reset()?;

            // Each episode with stored releases is set to its effective date, or
            // cleared when none of its releases qualify under the current priority
            // and filters.
            for (episode_id, releases) in by_episode {
                let aired = filters.effective_aired(&releases, &priority);
                s.set_episode_aired_by_id.execute((aired, episode_id))?;
            }

            Ok(())
        });

        result.await
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn create_movie(
        &self,
        id: MovieId,
        title: &str,
        release_date: Option<Timestamp>,
        overview: &str,
    ) -> Result<()> {
        let title = title.to_owned();
        let overview = overview.to_owned();

        let result = self.transaction(move |s| {
            s.insert_movie.execute((id, release_date))?;

            // Store the placeholder title/overview under the default locale so the
            // movie has something to show before its first sync; sync replaces these.
            if !title.trim().is_empty() {
                s.insert_movie_string.execute((
                    id,
                    api::Locale::DEFAULT,
                    api::StringKind::Title,
                    &title[..],
                ))?;
            }
            if !overview.trim().is_empty() {
                s.insert_movie_string.execute((
                    id,
                    api::Locale::DEFAULT,
                    api::StringKind::Overview,
                    &overview[..],
                ))?;
            }
            Ok(())
        });

        result.await
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
        let slug = slug.map(str::to_owned);
        let remote = remote.clone();

        self.transaction(move |s| s.add_movie_remote(movie_id, slug.as_deref(), &remote))
            .await
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
    pub(crate) async fn media_items(&self, user: UserId) -> Result<Vec<api::MediaItem>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out: Vec<api::MediaItem> = Vec::new();

            let config = s.user_language(user)?;

            // Movies. Keyed separately from shows so the raw ids can't collide.
            {
                let base = out.len();
                let mut id_to_idx: HashMap<u64, usize> = HashMap::new();

                let mut stmt = s.list_movie_items.bind((user,))?;

                while let Some(r) = stmt.next()? {
                    let strings =
                        api::Translations::new(r.language.0.or(r.default_language).or(config));
                    let item = media_item_from_row(r, api::MediaKind::Movies, strings);
                    id_to_idx.insert(item.id, out.len());
                    out.push(item);
                }

                stmt.reset()?;

                let mut stmt = s.list_all_movie_strings.query()?;

                while let Some(r) = stmt.next()? {
                    if let Some(&i) = id_to_idx.get(&r.movie_id.get())
                        && let Some(o) = out.get_mut(i)
                    {
                        o.strings.insert(r.kind, r.language, &r.text);
                    }
                }

                stmt.reset()?;

                for item in &mut out[base..] {
                    let id = MovieId::new(item.id);
                    item.poster = s.image.image_for_movie(id, ImageKind::Poster)?;
                    item.banner = s.image.image_for_movie(id, ImageKind::Banner)?;
                    item.backdrop = s.image.image_for_movie(id, ImageKind::Backdrop)?;
                }

                let mut stmt = s.last_watched_movies.bind((user,))?;

                while let Some(r) = stmt.next()? {
                    if let Some(&i) = id_to_idx.get(&r.movie_id.get())
                        && let Some(o) = out.get_mut(i)
                    {
                        o.last_watched_at = Some(r.last_watched);
                    }
                }

                stmt.reset()?;

                let mut stmt = s.list_all_movie_remotes.query()?;

                while let Some(r) = stmt.next()? {
                    if let Some(&i) = id_to_idx.get(&r.movie_id.get())
                        && let Some(o) = out.get_mut(i)
                    {
                        o.remotes.push(api::RemoteEntry {
                            id: r.id,
                            slug: r.slug,
                            remote: Remote::new(r.source, r.value),
                            enabled: r.enabled,
                            priority: r.priority,
                            sync_kinds: r.sync_kinds,
                            cache: parse_remote_cache(r.cache),
                        });
                    }
                }

                stmt.reset()?;
            }

            // Shows.
            {
                let base = out.len();
                let mut id_to_idx: HashMap<u64, usize> = HashMap::new();

                let mut stmt = s.list_show_items.bind((user,))?;

                while let Some(r) = stmt.next()? {
                    let strings =
                        api::Translations::new(r.language.0.or(r.default_language).or(config));
                    let item = media_item_from_row(r, api::MediaKind::Shows, strings);
                    id_to_idx.insert(item.id, out.len());
                    out.push(item);
                }

                stmt.reset()?;

                let mut stmt = s.list_all_show_strings.query()?;

                while let Some(r) = stmt.next()? {
                    if let Some(&i) = id_to_idx.get(&r.show_id.get())
                        && let Some(o) = out.get_mut(i)
                    {
                        o.strings.insert(r.kind, r.language, &r.text);
                    }
                }

                stmt.reset()?;

                for item in &mut out[base..] {
                    let id = ShowId::new(item.id);
                    item.poster = s.image.image_for_show(id, ImageKind::Poster)?;
                    item.banner = s.image.image_for_show(id, ImageKind::Banner)?;
                    item.backdrop = s.image.image_for_show(id, ImageKind::Backdrop)?;
                }

                let mut stmt = s.last_watched_shows.bind((user,))?;

                while let Some(r) = stmt.next()? {
                    if let Some(&i) = id_to_idx.get(&r.show_id.get())
                        && let Some(o) = out.get_mut(i)
                    {
                        o.last_watched_at = Some(r.last_watched);
                    }
                }

                stmt.reset()?;

                let mut stmt = s.unwatched_shows.bind((user,))?;

                while let Some(r) = stmt.next()? {
                    if let Some(&i) = id_to_idx.get(&r.show_id.get())
                        && let Some(o) = out.get_mut(i)
                    {
                        o.unwatched_episodes = u32::try_from(r.unwatched).unwrap_or(u32::MAX);
                    }
                }

                stmt.reset()?;

                let mut stmt = s
                    .next_episodes
                    .bind((user, None::<ShowId>, Timestamp::now()))?;

                while let Some(r) = stmt.next()? {
                    if let Some(&i) = id_to_idx.get(&r.show_id.get())
                        && let Some(o) = out.get_mut(i)
                    {
                        if r.special {
                            o.next_specials = true;
                        } else {
                            o.next_regular = true;
                        }
                    }
                }

                stmt.reset()?;

                let mut stmt = s.list_all_show_remotes.query()?;

                while let Some(r) = stmt.next()? {
                    if let Some(&i) = id_to_idx.get(&r.show_id.get())
                        && let Some(o) = out.get_mut(i)
                    {
                        o.remotes.push(api::RemoteEntry {
                            id: r.id,
                            slug: r.slug,
                            remote: Remote::new(r.source, r.value),
                            enabled: r.enabled,
                            priority: r.priority,
                            sync_kinds: r.sync_kinds,
                            cache: parse_remote_cache(r.cache),
                        });
                    }
                }

                stmt.reset()?;
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movies(&self, user: Option<UserId>) -> Result<Vec<api::Movie>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out: Vec<api::Movie> = Vec::new();
            let mut id_to_idx: HashMap<MovieId, usize> = HashMap::new();

            let cfg = s.user_language(user)?;

            let mut stmt = s.list_movies.bind((user,))?;

            while let Some(r) = stmt.next()? {
                id_to_idx.insert(r.id, out.len());
                let strings = api::Translations::new(r.language.0.or(r.default_language).or(cfg));
                out.push(movie_from_row(r, strings));
            }

            stmt.reset()?;

            let mut stmt = s.list_all_movie_strings.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&r.movie_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.strings.insert(r.kind, r.language, &r.text);
                }
            }

            stmt.reset()?;

            for movie in &mut out {
                movie.banner = s.image.image_for_movie(movie.id, ImageKind::Banner)?;
                movie.poster = s.image.image_for_movie(movie.id, ImageKind::Poster)?;
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
                        cache: parse_remote_cache(r.cache),
                    });
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_movie_images.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&r.movie_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.images.push(movie_image_from_row(r));
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_all_movie_image_selections.query()?;

            while let Some(r) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&r.movie_id)
                    && let Some(o) = out.get_mut(i)
                {
                    apply_movie_image_selection(
                        o,
                        ImageSelectionRow {
                            kind: r.kind,
                            source: r.source,
                            path: r.path,
                            width: r.width,
                            height: r.height,
                            user_selected: r.user_selected,
                        },
                    );
                }
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_by_id(
        &self,
        user: Option<UserId>,
        id: MovieId,
    ) -> Result<Option<api::Movie>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let Some(r) = s.movie_by_id.bind((user, id))?.first()? else {
                return Ok(None);
            };

            let movie_id = r.id;
            let cfg = s.user_language(user)?;
            let strings = s.translations.movie(user, movie_id, cfg)?;
            let mut movie = movie_from_row(r, strings);

            movie.remotes = s.movie_remotes(movie_id)?;

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
                    source: r.source,
                    country: r.country,
                    release_type: r.release_type,
                    timestamp: r.timestamp,
                });
            }

            stmt.reset()?;

            movie.pending = s.select_pending_movie.bind((user, movie_id))?.first()?;
            movie.poster = s.image.image_for_movie(movie_id, ImageKind::Poster)?;
            movie.banner = s.image.image_for_movie(movie_id, ImageKind::Banner)?;

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
    pub(crate) async fn shows_by_remote_id(
        &self,
        user: Option<UserId>,
        remote: &Remote,
    ) -> Result<Option<api::Show>> {
        let remote = remote.clone();
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let Some(row) = s
                .shows_by_remote
                .bind((user, remote.source(), remote.value()))?
                .first()?
            else {
                return Ok(None);
            };

            let show_id = row.id;
            let config = s.user_language(user)?;
            let strings = s.translations.show(user, show_id, config)?;
            let mut show = show_from_row(row, strings);

            show.remotes = s.show_remotes(show_id)?;

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

            show.poster = s.image.image_for_show(show_id, ImageKind::Poster)?;
            show.banner = s.image.image_for_show(show_id, ImageKind::Banner)?;
            Ok(Some(show))
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_by_remote_id(
        &self,
        user: Option<UserId>,
        remote: &Remote,
    ) -> Result<Option<api::Movie>> {
        let remote = remote.clone();
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let Some(row) = s
                .movie_by_remote
                .bind((user, remote.source(), remote.value()))?
                .first()?
            else {
                return Ok(None);
            };

            let movie_id = row.id;
            let cfg = s.user_language(user)?;
            let strings = s.translations.movie(user, movie_id, cfg)?;
            let mut movie = movie_from_row(row, strings);

            movie.remotes = s.movie_remotes(movie_id)?;

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

            movie.poster = s.image.image_for_movie(movie_id, ImageKind::Poster)?;
            movie.banner = s.image.image_for_movie(movie_id, ImageKind::Banner)?;

            Ok(Some(movie))
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
    pub(crate) async fn set_movie_tracked(
        &self,
        user: UserId,
        id: MovieId,
        tracked: bool,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            if tracked {
                s.track_movie.execute((user, id))?;
            } else {
                s.untrack_movie.execute((user, id))?;
            }

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

    /// Replace a movie remote's cached conditional-request state (JSON), or clear
    /// it with `None`.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_remote_cache(
        &self,
        remote_id: RemoteId,
        cache: Option<String>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_movie_remote_cache.execute((cache, remote_id))?;
            Ok(())
        });

        result.await?
    }

    /// Set remote priority to match the given order (first = highest priority).
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn reorder_movie_remotes(&self, remote_ids: Vec<RemoteId>) -> Result<()> {
        let result = self.transaction(move |s| {
            for (idx, id) in remote_ids.iter().enumerate() {
                s.set_movie_remote_priority.execute((idx as i32, *id))?;
            }
            Ok(())
        });

        result.await
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_language(
        &self,
        user: UserId,
        id: MovieId,
        language: api::Locale,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result =
            spawn_blocking(move || s.set_movie_pref(user, id, PreferenceKey::Language, language));

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_movie_release_filters(
        &self,
        id: MovieId,
        release_filters: Option<api::FilterRules>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let text = release_filters.as_ref().map(config::encode_filter_rules);
            s.set_movie_release_filters.execute((text, id))?;
            Ok(())
        });

        result.await?
    }

    /// Load a full person (localized name/biography, profile, and remotes with
    /// their per-remote [`api::RemoteCache`]) for the detail page and for sync.
    pub(crate) async fn person_by_id(
        &self,
        user: Option<UserId>,
        person_id: PersonId,
    ) -> Result<Option<api::Person>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let Some(row) = s.person_row.bind((person_id,))?.first()? else {
                return Ok(None);
            };

            let config = s.user_language(user)?;
            // Resolve strings against the configured display language, falling back
            // to the person's original language so a name/biography still shows when
            // no global language is set (mirrors show/movie title resolution).
            let locale = config.or(row.default_language);
            let mut name = api::Translations::new(locale);
            let mut biography = api::Translations::new(locale);

            let mut stmt = s.list_person_strings.bind((person_id,))?;
            while let Some((language, kind, text)) = stmt.next()? {
                match kind {
                    api::StringKind::Overview => biography.insert(kind, language, &text),
                    // Everything else (name) resolves against the name translations.
                    _ => name.insert(kind, language, &text),
                }
            }
            stmt.reset()?;

            let profile = s
                .person_profile
                .bind((person_id,))?
                .first()?
                .map(|p| api::Image::new_with_dims(p.source, &p.path, p.width, p.height));

            let mut remotes = Vec::new();
            let mut stmt = s.list_person_remotes.bind((person_id,))?;
            while let Some(r) = stmt.next()? {
                remotes.push(api::RemoteEntry {
                    id: r.id,
                    slug: r.slug,
                    remote: Remote::new(r.source, r.value),
                    enabled: r.enabled,
                    priority: r.priority,
                    sync_kinds: r.sync_kinds,
                    cache: parse_remote_cache(r.cache),
                });
            }
            stmt.reset()?;

            Ok(Some(api::Person {
                id: person_id,
                name,
                biography,
                profile,
                department: row.department,
                remotes,
                last_synced_at: row.last_synced_at,
            }))
        });

        result.await?
    }

    /// People that have never been synced (first) or are older than `interval`,
    /// with a best-effort display name, capped at `limit`.
    pub(crate) async fn people_needing_sync(
        &self,
        interval_hours: u32,
    ) -> Result<Vec<(PersonId, Option<String>)>> {
        let cutoff = cutoff_timestamp(interval_hours);
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.people_needing_sync.bind((cutoff,))?;

            while let Some(r) = stmt.next()? {
                out.push((r.id, r.name));
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// Replace a person's own data from its sync: department/imdb, per-language
    /// name+biography, ranked profile images, then mark synced and store the ETag.
    /// Replace a person's own data from its sync: department/imdb, per-language
    /// name+biography, ranked profile images, then mark synced. The per-remote
    /// [`api::RemoteCache`] is flushed separately via [`Self::set_person_remote_cache`].
    pub(crate) async fn persist_person_sync(
        &self,
        person_id: PersonId,
        department: Option<String>,
        default_language: api::Locale,
        strings: Vec<(api::Locale, api::StringKind, String)>,
        images: Vec<(f64, Image)>,
        now: Timestamp,
    ) -> Result<()> {
        let result = self.transaction(move |s| {
            s.update_person
                .execute((department, default_language, person_id))?;

            s.clear_person_strings.execute((person_id,))?;
            for (language, kind, text) in strings {
                s.insert_person_string
                    .execute((person_id, language, kind, text))?;
            }

            s.clear_person_images.execute((person_id,))?;
            for (rank, (score, image)) in images.into_iter().enumerate() {
                s.insert_person_image.execute((
                    ImageId::random(),
                    person_id,
                    ImageKind::Profile,
                    image.key().source(),
                    image.key().path(),
                    image.width(),
                    image.height(),
                    rank as u32,
                    Some(score),
                ))?;
            }

            s.mark_person_synced.execute((now, person_id))?;
            Ok(())
        });

        result.await
    }

    /// Mark a person synced without changing its data (unchanged / skipped path).
    pub(crate) async fn mark_person_synced(
        &self,
        person_id: PersonId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.mark_person_synced.execute((now, person_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn add_person_remote(
        &self,
        person_id: PersonId,
        slug: Option<&str>,
        remote: &Remote,
    ) -> Result<()> {
        let remote = remote.clone();
        let slug = slug.map(str::to_owned);
        let priority = default_remote_priority(*remote.source(), &self.load_config().await?);
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_person_remote.execute((
                RemoteId::random(),
                slug,
                person_id,
                remote.source(),
                remote.value(),
                true,
                priority,
                None::<api::SyncKindSet>,
            ))
        });

        result.await??;
        Ok(())
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn remove_person_remote(&self, remote_id: RemoteId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || s.delete_person_remote.execute((remote_id,)));

        result.await??;
        Ok(())
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn update_person_remote(
        &self,
        remote_id: RemoteId,
        slug: Option<&str>,
        remote: &Remote,
    ) -> Result<()> {
        let slug = slug.map(str::to_owned);
        let remote = remote.clone();
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.update_person_remote.execute((
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
    pub(crate) async fn set_person_remote_enabled(
        &self,
        remote_id: RemoteId,
        enabled: bool,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_person_remote_enabled.execute((enabled, remote_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_person_remote_sync_kinds(
        &self,
        remote_id: RemoteId,
        sync_kinds: Option<api::SyncKindSet>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_person_remote_sync_kinds
                .execute((sync_kinds, remote_id))?;
            Ok(())
        });

        result.await?
    }

    /// Replace a person remote's cached conditional-request state (JSON), or clear
    /// it with `None`.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_person_remote_cache(
        &self,
        remote_id: RemoteId,
        cache: Option<String>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.set_person_remote_cache.execute((cache, remote_id))?;
            Ok(())
        });

        result.await?
    }

    /// Set person remote priority to match the given order (first = highest).
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn reorder_person_remotes(&self, remote_ids: Vec<RemoteId>) -> Result<()> {
        let result = self.transaction(move |s| {
            for (idx, id) in remote_ids.iter().enumerate() {
                s.set_person_remote_priority.execute((idx as i32, *id))?;
            }
            Ok(())
        });

        result.await
    }

    /// Delete a person and everything derived from it: its remotes (no owner FK, so
    /// removed explicitly) and, via `ON DELETE CASCADE`, its strings, profile images
    /// and credits. The person re-seeds on the next credited show/movie sync.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn delete_person(&self, person_id: PersonId) -> Result<()> {
        let result = self.transaction(move |s| {
            s.delete_person_remotes.execute((person_id,))?;
            s.delete_person.execute((person_id,))?;
            Ok(())
        });

        result.await
    }

    /// The people list view: name, profile, department and total credit count.
    pub(crate) async fn list_persons(&self, user: UserId) -> Result<Vec<api::PersonItem>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let config = s.user_language(user)?;
            let mut out: Vec<api::PersonItem> = Vec::new();
            let mut id_to_idx: HashMap<PersonId, usize> = HashMap::new();

            let mut stmt = s.list_people.query()?;
            while let Some(r) = stmt.next()? {
                id_to_idx.insert(r.id, out.len());
                out.push(api::PersonItem {
                    id: r.id,
                    name: api::Translations::new(config.or(r.default_language)),
                    profile: None,
                    department: r.department,
                    credit_count: 0,
                });
            }
            stmt.reset()?;

            let mut stmt = s.list_all_person_names.query()?;
            while let Some((person_id, language, text)) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&person_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.name.insert(api::StringKind::Title, language, &text);
                }
            }
            stmt.reset()?;

            // Rows are ordered by (person_id, rank, id): the first per person wins.
            let mut stmt = s.list_all_person_profiles.query()?;
            while let Some(r) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&r.person_id)
                    && let Some(o) = out.get_mut(i)
                    && o.profile.is_none()
                {
                    o.profile = Some(api::Image::new_with_dims(
                        r.source, &r.path, r.width, r.height,
                    ));
                }
            }
            stmt.reset()?;

            let mut stmt = s.list_person_credit_counts.query()?;
            while let Some((person_id, n)) = stmt.next()? {
                if let Some(&i) = id_to_idx.get(&person_id)
                    && let Some(o) = out.get_mut(i)
                {
                    o.credit_count = n;
                }
            }
            stmt.reset()?;

            Ok(out)
        });

        result.await?
    }

    /// The shows and movies a person is credited on, for the detail page.
    pub(crate) async fn list_person_credits(
        &self,
        user: UserId,
        person_id: PersonId,
    ) -> Result<Vec<api::PersonCredit>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let config = s.user_language(user)?;

            let mut out: Vec<api::PersonCredit> = Vec::new();

            // Shows. Strings are collected raw (per credit) so each credit resolves
            // its title/character against its own owner's display locale below.
            let mut titles: HashMap<CreditId, Vec<(api::Locale, String)>> = HashMap::new();
            let mut stmt = s.list_person_show_titles.bind((person_id,))?;
            while let Some((credit_id, language, text)) = stmt.next()? {
                titles.entry(credit_id).or_default().push((language, text));
            }
            stmt.reset()?;

            let mut characters: HashMap<CreditId, Vec<(api::Locale, String)>> = HashMap::new();
            let mut stmt = s.list_person_show_credit_strings.bind((person_id,))?;
            while let Some((credit_id, language, text)) = stmt.next()? {
                characters
                    .entry(credit_id)
                    .or_default()
                    .push((language, text));
            }
            stmt.reset()?;

            let mut stmt = s.list_person_show_credits.bind((person_id,))?;
            while let Some(r) = stmt.next()? {
                let owner = api::CreditOwner::Show(ShowId::new(r.owner_id as u64));
                let locale = config.or(r.owner_language);
                out.push(person_credit_from_row(
                    r,
                    owner,
                    &mut titles,
                    &mut characters,
                    locale,
                ));
            }
            stmt.reset()?;

            // Movies.
            let mut titles: HashMap<CreditId, Vec<(api::Locale, String)>> = HashMap::new();
            let mut stmt = s.list_person_movie_titles.bind((person_id,))?;
            while let Some((credit_id, language, text)) = stmt.next()? {
                titles.entry(credit_id).or_default().push((language, text));
            }
            stmt.reset()?;

            let mut characters: HashMap<CreditId, Vec<(api::Locale, String)>> = HashMap::new();
            let mut stmt = s.list_person_movie_credit_strings.bind((person_id,))?;
            while let Some((credit_id, language, text)) = stmt.next()? {
                characters
                    .entry(credit_id)
                    .or_default()
                    .push((language, text));
            }
            stmt.reset()?;

            let mut stmt = s.list_person_movie_credits.bind((person_id,))?;
            while let Some(r) = stmt.next()? {
                let owner = api::CreditOwner::Movie(MovieId::new(r.owner_id as u64));
                let locale = config.or(r.owner_language);
                out.push(person_credit_from_row(
                    r,
                    owner,
                    &mut titles,
                    &mut characters,
                    locale,
                ));
            }
            stmt.reset()?;

            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn list_show_credits(
        &self,
        user: UserId,
        show_id: ShowId,
    ) -> Result<Vec<Credit>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let config = s.user_language(user)?;
            let (language, default) = s
                .translations
                .show_locales
                .bind((Some(user), show_id))?
                .first()?
                .map(|r| (r.language.0, r.default_language))
                .unwrap_or_default();
            let locale = language.or(config).or(default);

            let mut names: HashMap<CreditId, api::Translations> = HashMap::new();
            let mut stmt = s.list_show_credit_names.bind((show_id,))?;

            while let Some((credit_id, string_locale, text)) = stmt.next()? {
                names
                    .entry(credit_id)
                    .or_insert_with(|| api::Translations::new(locale))
                    .insert(api::StringKind::Title, string_locale, &text);
            }

            stmt.reset()?;

            let mut characters: HashMap<CreditId, api::Translations> = HashMap::new();
            let mut stmt = s.list_show_credit_strings.bind((show_id,))?;

            while let Some((credit_id, string_locale, text)) = stmt.next()? {
                characters
                    .entry(credit_id)
                    .or_insert_with(|| api::Translations::new(locale))
                    .insert(api::StringKind::Character, string_locale, &text);
            }

            stmt.reset()?;

            let mut out = Vec::new();
            let mut stmt = s.list_show_credits.bind((show_id,))?;

            while let Some(r) = stmt.next()? {
                out.push(credit_from_row(r, &mut names, &mut characters, locale));
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    pub(crate) async fn list_movie_credits(
        &self,
        user: UserId,
        movie_id: MovieId,
    ) -> Result<Vec<Credit>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let config = s.user_language(user)?;
            let (language, default) = s
                .translations
                .movie_locales
                .bind((Some(user), movie_id))?
                .first()?
                .map(|r| (r.language.0, r.default_language))
                .unwrap_or_default();
            let locale = language.or(config).or(default);

            let mut names: HashMap<CreditId, api::Translations> = HashMap::new();
            let mut stmt = s.list_movie_credit_names.bind((movie_id,))?;

            while let Some((credit_id, string_locale, text)) = stmt.next()? {
                names
                    .entry(credit_id)
                    .or_insert_with(|| api::Translations::new(locale))
                    .insert(api::StringKind::Title, string_locale, &text);
            }

            stmt.reset()?;

            let mut characters: HashMap<CreditId, api::Translations> = HashMap::new();
            let mut stmt = s.list_movie_credit_strings.bind((movie_id,))?;

            while let Some((credit_id, string_locale, text)) = stmt.next()? {
                characters
                    .entry(credit_id)
                    .or_insert_with(|| api::Translations::new(locale))
                    .insert(api::StringKind::Character, string_locale, &text);
            }

            stmt.reset()?;

            let mut out = Vec::new();
            let mut stmt = s.list_movie_credits.bind((movie_id,))?;

            while let Some(r) = stmt.next()? {
                out.push(credit_from_row(r, &mut names, &mut characters, locale));
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// Read the translated strings stored for a show.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn show_translations(&self, id: ShowId) -> Result<Vec<api::Translation>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.translations.list_show_strings.bind((id,))?;

            while let Some((language, kind, text)) = stmt.next()? {
                out.push(api::Translation {
                    language,
                    kind,
                    text,
                });
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// Read the translated strings stored for a movie.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_translations(&self, id: MovieId) -> Result<Vec<api::Translation>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.translations.list_movie_strings.bind((id,))?;

            while let Some((language, kind, text)) = stmt.next()? {
                out.push(api::Translation {
                    language,
                    kind,
                    text,
                });
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// Read the translated strings stored for an episode.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn episode_translations(
        &self,
        id: EpisodeId,
    ) -> Result<Vec<api::Translation>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.translations.list_episode_strings.bind((id,))?;

            while let Some((language, kind, text)) = stmt.next()? {
                out.push(api::Translation {
                    language,
                    kind,
                    text,
                });
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// Read the translated strings stored for a season.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn season_translations(&self, id: SeasonId) -> Result<Vec<api::Translation>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.list_season_strings.bind((id,))?;

            while let Some((language, kind, text)) = stmt.next()? {
                out.push(api::Translation {
                    language,
                    kind,
                    text,
                });
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// Selects the given image for its owning entity + kind, replacing any
    /// prior selection. Returns which entity owns the image.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn select_image(&self, id: ImageId) -> Result<api::ImageOwner> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            // Ids are globally unique, so at most one candidate table owns this id.
            let show = s.show_image_owner_by_id.bind((id,))?.first()?;
            if let Some((show_id, kind)) = show {
                // A manual pick is a user selection: protect it from sync.
                s.set_show_image_selection
                    .execute((show_id, kind, id, true))?;
                return Ok(api::ImageOwner::Show(show_id));
            }

            let movie = s.movie_image_owner_by_id.bind((id,))?.first()?;
            if let Some((movie_id, kind)) = movie {
                s.set_movie_image_selection
                    .execute((movie_id, kind, id, true))?;
                return Ok(api::ImageOwner::Movie(movie_id));
            }

            let season = s.season_image_owner_by_id.bind((id,))?.first()?;
            if let Some((season_id, kind)) = season {
                s.set_season_image_selection
                    .execute((season_id, kind, id))?;
                return Ok(api::ImageOwner::Season(season_id));
            }

            anyhow::bail!("Expected image to exist")
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

    /// Select the configured-order default (lowest-rank) stored graphic for the
    /// given show kind(s) - one kind when `kind` is `Some`, else every kind that
    /// has an image. `user_selected` records whether this is an explicit user
    /// choice (pick-best) or a reset to automatic management.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn pick_best_show_image(
        &self,
        show_id: ShowId,
        kind: Option<ImageKind>,
        user_selected: bool,
    ) -> Result<()> {
        let result = self.transaction(move |s| {
            let kinds = match kind {
                Some(kind) => vec![kind],
                None => {
                    let mut kinds = Vec::new();
                    let mut stmt = s.show_image_kinds.bind((show_id,))?;
                    while let Some(kind) = stmt.next()? {
                        kinds.push(kind);
                    }
                    stmt.reset()?;
                    kinds
                }
            };

            for kind in kinds {
                let best = s.best_show_image.bind((show_id, kind))?.first()?;

                if let Some(image_id) = best {
                    s.set_show_image_selection
                        .execute((show_id, kind, image_id, user_selected))?;
                }
            }

            Ok(())
        });

        result.await
    }

    /// Same as [`pick_best_show_image`], for a movie.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn pick_best_movie_image(
        &self,
        movie_id: MovieId,
        kind: Option<ImageKind>,
        user_selected: bool,
    ) -> Result<()> {
        let result = self.transaction(move |s| {
            let kinds = match kind {
                Some(kind) => vec![kind],
                None => {
                    let mut kinds = Vec::new();
                    let mut stmt = s.movie_image_kinds.bind((movie_id,))?;
                    while let Some(kind) = stmt.next()? {
                        kinds.push(kind);
                    }
                    stmt.reset()?;
                    kinds
                }
            };

            for kind in kinds {
                let best = s.best_movie_image.bind((movie_id, kind))?.first()?;

                if let Some(image_id) = best {
                    s.set_movie_image_selection.execute((
                        movie_id,
                        kind,
                        image_id,
                        user_selected,
                    ))?;
                }
            }

            Ok(())
        });

        result.await
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn season_images_by_id(
        &self,
        season_id: SeasonId,
    ) -> Result<Vec<api::MediaImage>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.list_season_images.bind((season_id,))?;

            while let Some(r) = stmt.next()? {
                out.push(image_from_row(r));
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn show_id_for_season(&self, season_id: SeasonId) -> Result<Option<ShowId>> {
        let mut s = self.inner.clone().shared().await?;
        spawn_blocking(move || Ok(s.show_id_for_season.bind((season_id,))?.first()?)).await?
    }

    /// The next episode in watch order of `show` in `scope`; see
    /// `next_episodes` for the rule.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn next_episode(
        &self,
        user: UserId,
        show: ShowId,
        scope: api::EpisodeScope,
        now: Timestamp,
    ) -> Result<Option<EpisodeId>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let special = scope == api::EpisodeScope::Specials;
            let mut stmt = s.next_episodes.bind((user, Some(show), now))?;

            while let Some(r) = stmt.next()? {
                if r.special == special {
                    return Ok(Some(r.episode_id));
                }
            }

            Ok(None)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn mark_watched(
        &self,
        user: UserId,
        id: WatchedId,
        kind: WatchedKind,
        mark_time: MarkTime,
        now: Timestamp,
    ) -> Result<api::Watched> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let (watched_id, timestamp) = match kind {
                WatchedKind::Episode { episode, .. } => {
                    let timestamp = s.episodes.episode_mark_time(episode, mark_time, now)?;

                    let key = s
                        .episode_natural_key
                        .bind((episode,))?
                        .first()?
                        .context("Expected episode to exist")?;

                    s.insert_watched_episode.execute((
                        id,
                        user,
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
                        MarkTime::At(ts) => ts,
                        MarkTime::WhenAired => s
                            .movie_released_by_id
                            .bind((movie,))?
                            .first()?
                            .flatten()
                            .context("Movie has no release date")?,
                    };

                    s.insert_watched_movie
                        .execute((id, user, timestamp, movie))?;
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

    #[cfg(any(feature = "import", test))]
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn insert_watched_episode(
        &self,
        user: UserId,
        id: WatchedId,
        timestamp: Timestamp,
        show_id: ShowId,
        season: api::SeasonNumber,
        episode: u32,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_watched_episode
                .execute((id, user, timestamp, show_id, season, episode))?;
            Ok(())
        });

        result.await?
    }

    #[cfg(any(feature = "import", test))]
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn insert_watched_movie(
        &self,
        user: UserId,
        id: WatchedId,
        timestamp: Timestamp,
        movie_id: MovieId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.insert_watched_movie
                .execute((id, user, timestamp, movie_id))?;
            Ok(())
        });

        result.await?
    }

    // --- backup export (read-only) ---

    /// Everything a backup holds, read inside one read transaction so rows
    /// written meanwhile cannot leave it inconsistent.
    #[tracing::instrument(skip(self))]
    pub(crate) async fn export_snapshot(&self) -> Result<ExportSnapshot> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            s.begin.execute(())?;
            let snapshot = s.export_snapshot();
            // Ends the read transaction even when a read failed, so the pooled
            // connection is not left pinned to this snapshot.
            s.commit.execute(())?;
            snapshot
        });

        result.await?
    }

    // --- backup import (idempotent; bool = inserted vs. ignored duplicate) ---

    /// Track a show for a user, creating a placeholder show for sync to fill in
    /// when it does not exist yet.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn import_tracked_show(&self, user: UserId, show: ShowId) -> Result<bool> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.ensure_show.execute((show,))?;
            Ok(s.import_tracked_show.bind((user, show))?.first()?.is_some())
        });

        result.await?
    }

    /// Track a movie for a user, creating a placeholder movie when missing.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn import_tracked_movie(&self, user: UserId, movie: MovieId) -> Result<bool> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.ensure_movie.execute((movie,))?;
            Ok(s.import_tracked_movie
                .bind((user, movie))?
                .first()?
                .is_some())
        });

        result.await?
    }

    /// Store a user's own preference unless the user already has one for `key`.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn import_user_preference(
        &self,
        user: UserId,
        key: PreferenceKey,
        value: String,
    ) -> Result<bool> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            Ok(s.import_user_config
                .bind((user, key, value))?
                .first()?
                .is_some())
        });

        result.await?
    }

    /// Store a user's preference for a show unless one exists for `key`,
    /// creating a placeholder show when missing.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn import_show_preference(
        &self,
        user: UserId,
        show: ShowId,
        key: PreferenceKey,
        value: String,
    ) -> Result<bool> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.ensure_show.execute((show,))?;
            Ok(s.import_user_show_config
                .bind((user, show, key, value))?
                .first()?
                .is_some())
        });

        result.await?
    }

    /// Store a user's preference for a movie unless one exists for `key`,
    /// creating a placeholder movie when missing.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn import_movie_preference(
        &self,
        user: UserId,
        movie: MovieId,
        key: PreferenceKey,
        value: String,
    ) -> Result<bool> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.ensure_movie.execute((movie,))?;
            Ok(s.import_user_movie_config
                .bind((user, movie, key, value))?
                .first()?
                .is_some())
        });

        result.await?
    }

    /// Insert a show remote under its original identifier, preserving its
    /// structure. Idempotent: a remote with the same identifier, or the same
    /// show, source and value, is left untouched.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn import_show_remote(&self, remote: ExportRemote<ShowId>) -> Result<bool> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let exists = s
                .show_remote_exists
                .bind((remote.id, remote.owner, remote.source, &remote.value))?
                .first()?
                .is_some();

            if exists {
                return Ok(false);
            }

            s.insert_show_remote.execute((
                remote.id,
                remote.slug,
                remote.owner,
                remote.source,
                remote.value,
                remote.enabled,
                remote.priority,
                remote.sync_kinds,
            ))?;

            Ok(true)
        });

        result.await?
    }

    /// Insert a movie remote under its original identifier, preserving its
    /// structure. Idempotent, as for shows.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn import_movie_remote(&self, remote: ExportRemote<MovieId>) -> Result<bool> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let exists = s
                .movie_remote_exists
                .bind((remote.id, remote.owner, remote.source, &remote.value))?
                .first()?
                .is_some();

            if exists {
                return Ok(false);
            }

            s.insert_movie_remote.execute((
                remote.id,
                remote.slug,
                remote.owner,
                remote.source,
                remote.value,
                remote.enabled,
                remote.priority,
                remote.sync_kinds,
            ))?;

            Ok(true)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn import_watched_episode(
        &self,
        user: UserId,
        id: WatchedId,
        timestamp: Timestamp,
        show_id: ShowId,
        season: SeasonNumber,
        episode: u32,
    ) -> Result<bool> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let exists = s.watched_episode_exists.bind((id,))?.first()?.is_some();

            if exists {
                return Ok(false);
            }

            s.insert_watched_episode
                .execute((id, user, timestamp, show_id, season, episode))?;
            Ok(true)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn import_watched_movie(
        &self,
        user: UserId,
        id: WatchedId,
        timestamp: Timestamp,
        movie_id: MovieId,
    ) -> Result<bool> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let exists = s.watched_movie_exists.bind((id,))?.first()?.is_some();

            if exists {
                return Ok(false);
            }

            s.insert_watched_movie
                .execute((id, user, timestamp, movie_id))?;
            Ok(true)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn move_watched_episode(
        &self,
        user: UserId,
        id: WatchedId,
        season: api::SeasonNumber,
        episode: u32,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.move_watched_episode
                .execute((season, episode, user, id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn orphaned_for_show(
        &self,
        user: UserId,
        show_id: ShowId,
    ) -> Result<Vec<api::OrphanedWatched>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_orphaned_for_show.bind((user, show_id))?;

            while let Some(r) = stmt.next()? {
                out.push(api::OrphanedWatched {
                    id: r.id,
                    timestamp: r.timestamp,
                    show_id: r.show_id,
                    season: r.season,
                    episode: r.episode,
                });
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn remove_watched(&self, user: UserId, id: WatchedId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_watched_episode.execute((user, id))?;
            s.delete_watched_movie.execute((user, id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn watched_for_episode(
        &self,
        user: UserId,
        episode_id: EpisodeId,
    ) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_watched_by_episode.bind((user, episode_id))?;

            while let Some(r) = stmt.next()? {
                out.push(watched_from_row(r)?);
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn watched_for_movie(
        &self,
        user: UserId,
        movie_id: MovieId,
    ) -> Result<Vec<api::Watched>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_watched_by_movie.bind((user, movie_id))?;

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
        user: UserId,
        show_id: api::ShowId,
        episode_id: api::EpisodeId,
        ts: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.upsert_pending_episode.execute((
                PendingId::random(),
                user,
                ts,
                show_id,
                episode_id,
            ))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn add_pending_movie(
        &self,
        user: UserId,
        movie_id: api::MovieId,
        ts: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.upsert_pending_movie
                .execute((PendingId::random(), user, ts, movie_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn remove_pending_episode(
        &self,
        user: UserId,
        show_id: api::ShowId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_pending_episode.execute((user, show_id))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn skip_pending_episode(
        &self,
        user: UserId,
        show_id: api::ShowId,
        episode_id: api::EpisodeId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let next = s.next_episode_after.bind((show_id, episode_id))?.first()?;

            match next {
                Some((next_id, aired)) => {
                    let ts = aired.unwrap_or(now).max(now);

                    s.upsert_pending_episode.execute((
                        PendingId::random(),
                        user,
                        ts,
                        show_id,
                        next_id,
                    ))?;
                }
                None => {
                    s.delete_pending_episode.execute((user, show_id))?;
                }
            }

            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn remove_pending_movie(
        &self,
        user: UserId,
        movie_id: api::MovieId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.delete_pending_movie.execute((user, movie_id))?;
            Ok(())
        });

        result.await?
    }

    /// Recompute a movie's effective release date and pending entry from its release filters, in a
    /// single exclusive transaction.
    ///
    /// The effective release date is the earliest release matching the filters; it is written back
    /// to `movies.release_date` so the displayed date reflects the settings (when no release matches
    /// the date is cleared). Pending is then updated for each user tracking the movie: one who has
    /// watched it is left to the watch flow; otherwise any qualifying release date (past or future)
    /// creates/updates their pending entry, and the absence of a qualifying release (e.g. filters
    /// changed so nothing matches) removes it. Future-dated pending rows stay dormant on the dashboard until their
    /// timestamp falls within the configured dashboard lookahead (`list_pending_before` filters
    /// `timestamp <= cutoff`).
    ///
    /// `default_filters` are the global release filters used when the movie has no override.
    #[tracing::instrument(skip(self, default_filters), ret(level = "trace"))]
    pub(crate) async fn update_movie_pending(
        &self,
        movie_id: MovieId,
        default_filters: api::FilterRules,
    ) -> Result<()> {
        let result = self.transaction(move |s| {
            // Resolve the current release date, which also confirms the movie exists.
            let Some(current_release_date) = s.movie_released_by_id.bind((movie_id,))?.first()?
            else {
                return Ok(());
            };

            let override_filters = s
                .movie_release_filters
                .bind((movie_id,))?
                .first()?
                .flatten();
            let override_filters = override_filters
                .as_deref()
                .and_then(config::decode_filter_rules);

            let releases = {
                let mut out = Vec::new();
                let mut stmt = s.list_movie_releases.bind((movie_id,))?;

                while let Some(r) = stmt.next()? {
                    out.push(api::MovieRelease {
                        source: r.source,
                        country: r.country,
                        release_type: r.release_type,
                        timestamp: r.timestamp,
                    });
                }

                stmt.reset()?;
                out
            };

            let effective = override_filters.as_ref().unwrap_or(&default_filters);
            let release = effective.earliest_release(&releases);

            if current_release_date != release {
                s.set_movie_release_date
                    .execute((release.as_ref(), movie_id))?;
            }

            for user in s.trackers_of_movie(movie_id)? {
                if s.has_watched_movie
                    .bind((user, movie_id))?
                    .first()?
                    .is_some()
                {
                    continue;
                }

                match release {
                    Some(ts) => {
                        s.upsert_pending_movie.execute((
                            PendingId::random(),
                            user,
                            ts,
                            movie_id,
                        ))?;
                    }
                    None => {
                        s.delete_pending_movie.execute((user, movie_id))?;
                    }
                }
            }

            Ok(())
        });

        result.await
    }

    /// Fill the pending slot for a show for every user tracking it, but ONLY for
    /// those without a pending episode. Called after sync upserts episodes.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn fill_pending_for_show(
        &self,
        show_id: api::ShowId,
        now: Timestamp,
    ) -> Result<()> {
        let result = self.transaction(move |s| {
            for user in s.trackers_of_show(show_id)? {
                s.fill_pending_for_show(user, show_id, now)?;
            }

            Ok(())
        });

        result.await
    }

    /// Fill one user's pending slot for a show, as [`Self::fill_pending_for_show`]
    /// does for every user tracking it.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn fill_pending_for_user_show(
        &self,
        user: UserId,
        show_id: api::ShowId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || s.fill_pending_for_show(user, show_id, now));

        result.await?
    }

    /// Fill the user's pending slot for a show with the episode after
    /// `episode_id`, but ONLY if it currently has no pending episode. Called
    /// after MarkWatched clears the old pending row.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn fill_pending_for_show_from(
        &self,
        user: UserId,
        show_id: api::ShowId,
        episode_id: api::EpisodeId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let already_has = s
                .has_pending_episode_for_show
                .bind((user, show_id))?
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
                .execute((PendingId::random(), user, now, show_id, next_id))?;

            Ok(())
        });

        result.await?
    }

    /// Like `fill_pending_for_show` but for bulk import: finds the first unwatched episode
    /// regardless of whether it has aired, and uses the actual aired timestamp rather than
    /// clamping to `now`. This preserves the episode's original air date as the pending
    /// timestamp so dashboard ordering reflects episode order rather than import time.
    #[cfg(feature = "import")]
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn fill_pending_for_show_import(
        &self,
        user: UserId,
        show_id: api::ShowId,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            let stmt = s.has_pending_episode_for_show.bind((user, show_id))?;

            let already_has = stmt.first()?.is_some();

            if already_has {
                return Ok(());
            }

            let stmt = s.first_unwatched_episode_for_show.bind((user, show_id))?;

            let Some(row) = stmt.first()? else {
                return Ok(());
            };

            let Some(ts) = row.aired else {
                return Ok(());
            };

            s.upsert_pending_episode
                .execute((PendingId::random(), user, ts, show_id, row.id))?;

            Ok(())
        });

        result.await?
    }

    /// Movies a user tracks that are not yet pending or watched by them, paired with their
    /// per-movie release filter override (raw JSON, `None` = use global default).
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_pending_candidates(
        &self,
    ) -> Result<Vec<(UserId, MovieId, Option<api::FilterRules>)>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.movie_pending_candidates.query()?;

            while let Some(r) = stmt.next()? {
                let release_filters = r
                    .release_filters
                    .as_deref()
                    .and_then(config::decode_filter_rules);

                out.push((r.user_id, r.id, release_filters));
            }

            stmt.reset()?;
            Ok(out)
        });

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
                    source: r.source,
                    country: r.country,
                    release_type: r.release_type,
                    timestamp: r.timestamp,
                });
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// The air dates recorded for a single episode, attributed to their source.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn episode_releases(&self, id: EpisodeId) -> Result<Vec<api::EpisodeRelease>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let mut stmt = s.list_episode_releases.bind((id,))?;

            while let Some((source, country, network, timestamp)) = stmt.next()? {
                out.push(api::EpisodeRelease {
                    source,
                    country,
                    network,
                    timestamp,
                });
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// The owning show of an episode, if it exists.
    async fn episode_show_id(&self, id: EpisodeId) -> Result<Option<ShowId>> {
        let mut s = self.inner.clone().shared().await?;
        let result = spawn_blocking(move || {
            Ok(s.episode_natural_key
                .bind((id,))?
                .first()?
                .map(|r| r.show_id))
        });
        result.await?
    }

    /// An episode's air-date releases as display rows, with `considered` and the
    /// grouping `label` (network, or `"Unknown"`) resolved against the show's
    /// air-date source priority and effective filters.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn episode_release_rows(
        &self,
        id: EpisodeId,
    ) -> Result<(Vec<api::ReleaseRow>, ShowId, Option<api::FilterRules>)> {
        let releases = self.episode_releases(id).await?;

        let Some(show_id) = self.episode_show_id(id).await? else {
            return Ok((Vec::new(), ShowId::new(0), None));
        };

        let Some(show) = self.show_by_id(None, show_id).await? else {
            return Ok((Vec::new(), show_id, None));
        };

        let config = self.load_config().await?;
        let priority = api::air_date_sources_by_priority(&show.remotes, &config);
        let filters = show.effective_air_date_filters(&config.air_date_filters);

        let mut rows: Vec<api::ReleaseRow> = releases
            .into_iter()
            .map(|r| {
                let considered = filters.air_date_considered(&r, &priority);
                let label = if r.network.is_empty() {
                    "Unknown".to_owned()
                } else {
                    r.network
                };

                api::ReleaseRow {
                    label,
                    source: r.source,
                    country: r.country,
                    timestamp: r.timestamp,
                    considered,
                }
            })
            .collect();

        rows.sort_by(|a, b| a.label.cmp(&b.label).then(a.timestamp.cmp(&b.timestamp)));
        Ok((rows, show_id, show.air_date_filters))
    }

    /// A movie's release-date override filters, if any.
    async fn movie_release_filters(&self, id: MovieId) -> Result<Option<api::FilterRules>> {
        let mut s = self.inner.clone().shared().await?;
        let result = spawn_blocking(move || {
            let text = s.movie_release_filters.bind((id,))?.first()?.flatten();
            Ok(text.as_deref().and_then(config::decode_filter_rules))
        });
        result.await?
    }

    /// A movie's releases as display rows, with `considered` and the grouping
    /// `label` (release type) resolved against the movie's effective release filters.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_release_rows(
        &self,
        id: MovieId,
    ) -> Result<(Vec<api::ReleaseRow>, Option<api::FilterRules>)> {
        let mut releases = self.movie_releases(id).await?;
        let override_filters = self.movie_release_filters(id).await?;
        let config = self.load_config().await?;
        let effective = override_filters.as_ref().unwrap_or(&config.release_filters);

        releases.sort_by(|a, b| {
            a.release_type
                .as_u32()
                .cmp(&b.release_type.as_u32())
                .then(a.timestamp.cmp(&b.timestamp))
        });

        let rows = releases
            .into_iter()
            .map(|r| {
                let considered = effective.release_accepted(&r);

                api::ReleaseRow {
                    label: r.release_type.as_str().to_owned(),
                    source: r.source,
                    country: r.country,
                    timestamp: r.timestamp,
                    considered,
                }
            })
            .collect();

        Ok((rows, override_filters))
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
    pub(crate) async fn shows_needing_sync(&self, interval_hours: u32) -> Result<Vec<api::Show>> {
        let cutoff = cutoff_timestamp(interval_hours);
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let s = &mut *s;

            let config = s.user_language(None)?;

            let mut out = Vec::new();
            let mut stmt = s.shows_needing_sync.bind((cutoff,))?;

            while let Some(r) = stmt.next()? {
                let strings = s.translations.show(None, r.id, config)?;
                out.push(show_from_row(r, strings));
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movies_needing_sync(&self, interval_hours: u32) -> Result<Vec<api::Movie>> {
        let cutoff = cutoff_timestamp(interval_hours);
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let s = &mut *s;

            let cfg = s.user_language(None)?;

            let mut out = Vec::new();

            let mut stmt = s.movies_needing_sync.bind((cutoff,))?;

            while let Some(r) = stmt.next()? {
                let strings = s.translations.movie(None, r.id, cfg)?;
                out.push(movie_from_row(r, strings));
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// Unified pending list replacing pending_episodes + pending_movies.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn pending(&self, user: UserId, now: Timestamp) -> Result<Vec<api::Pending>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();

            let s = &mut *s;

            let config = s.user_language(user)?;

            let mut stmt = s.list_pending_before.bind((user, now))?;

            'outer: while let Some(r) = stmt.next()? {
                let pending = 'pending: {
                    if let Some(episode_id) = r.episode_id {
                        let detail = s.pending_episode_detail.bind((user, episode_id))?.first()?;

                        let Some(d) = detail else {
                            continue 'outer;
                        };

                        let poster = s.image.image_for_show(d.show_id, ImageKind::Poster)?;
                        let banner = s.image.image_for_show(d.show_id, ImageKind::Banner)?;
                        let backdrop = s.image.image_for_show(d.show_id, ImageKind::Backdrop)?;
                        let season_poster =
                            s.image
                                .image_for_season(d.show_id, d.season, ImageKind::Poster)?;
                        let season_banner =
                            s.image
                                .image_for_season(d.show_id, d.season, ImageKind::Banner)?;

                        let show_title = s
                            .translations
                            .show(user, d.show_id, config)?
                            .title()
                            .map(str::to_owned);

                        let episode_name = s
                            .translations
                            .episode(episode_id, d.language.0, d.default_language, config)?
                            .title()
                            .map(str::to_owned);

                        break 'pending api::Pending {
                            info: api::PendingInfo::Episode {
                                show_id: d.show_id,
                                episode_id,
                                show: show_title,
                                episode: episode_name,
                                season: d.season,
                                number: d.number,
                            },
                            aired: d.aired,
                            timestamp: r.timestamp,
                            poster,
                            banner,
                            season_poster,
                            season_banner,
                            backdrop,
                        };
                    }

                    if let Some(movie_id) = r.movie_id {
                        let detail = s.pending_movie_detail.bind((user, movie_id))?.first()?;

                        let Some(d) = detail else {
                            continue 'outer;
                        };

                        let poster = s.image.image_for_movie(movie_id, ImageKind::Poster)?;
                        let banner = s.image.image_for_movie(movie_id, ImageKind::Banner)?;
                        let backdrop = s.image.image_for_movie(movie_id, ImageKind::Backdrop)?;

                        let title = s
                            .translations
                            .movie(user, movie_id, config)?
                            .title()
                            .map(str::to_owned);

                        break 'pending api::Pending {
                            info: api::PendingInfo::Movie {
                                movie: movie_id,
                                title,
                            },
                            aired: d.release_date,
                            timestamp: r.timestamp,
                            poster,
                            banner,
                            season_poster: None,
                            season_banner: None,
                            backdrop,
                        };
                    }

                    continue 'outer;
                };

                out.push(pending);
            }

            stmt.reset()?;
            Ok(out)
        });

        result.await?
    }

    /// Build the single denormalized [`api::Pending`] for one entry, or `None` if
    /// the media is no longer tracked or has no pending row. Used to emit granular
    /// pending updates without reloading the whole list.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    /// What is pending for the show or movie of `kind`, so that marking it
    /// watched can be undone.
    pub(crate) async fn pending_before(
        &self,
        user: UserId,
        kind: api::WatchedKind,
    ) -> Result<api::PendingBefore> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || match kind {
            api::WatchedKind::Episode { show, .. } => {
                let pending = s.pending_for_show.bind((user, show))?.first()?;

                Ok(match pending {
                    Some((episode, timestamp)) => {
                        api::PendingBefore::Episode { episode, timestamp }
                    }
                    None => api::PendingBefore::None,
                })
            }
            api::WatchedKind::Movie { movie } => {
                let pending = s.pending_timestamp_for_movie.bind((user, movie))?.first()?;

                Ok(match pending {
                    Some((timestamp,)) => api::PendingBefore::Movie { timestamp },
                    None => api::PendingBefore::None,
                })
            }
        });

        result.await?
    }

    pub(crate) async fn pending_entry(
        &self,
        user: UserId,
        kind: api::PendingKind,
    ) -> Result<Option<api::Pending>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || match kind {
            api::PendingKind::Episode { show, episode } => {
                let Some((timestamp,)) = s
                    .pending_timestamp_for_episode
                    .bind((user, episode))?
                    .first()?
                else {
                    return Ok(None);
                };

                let Some(d) = s.pending_episode_detail.bind((user, episode))?.first()? else {
                    return Ok(None);
                };

                let config = s.user_language(user)?;
                let poster = s.image.image_for_show(d.show_id, ImageKind::Poster)?;
                let banner = s.image.image_for_show(d.show_id, ImageKind::Banner)?;
                let backdrop = s.image.image_for_show(d.show_id, ImageKind::Backdrop)?;
                let season_poster =
                    s.image
                        .image_for_season(d.show_id, d.season, ImageKind::Poster)?;
                let season_banner =
                    s.image
                        .image_for_season(d.show_id, d.season, ImageKind::Banner)?;

                let show_title = s
                    .translations
                    .show(user, d.show_id, config)?
                    .title()
                    .map(str::to_owned);

                let episode_name = s
                    .translations
                    .episode(episode, d.language.0, d.default_language, config)?
                    .title()
                    .map(str::to_owned);

                Ok(Some(api::Pending {
                    info: api::PendingInfo::Episode {
                        show_id: show,
                        episode_id: episode,
                        show: show_title,
                        episode: episode_name,
                        season: d.season,
                        number: d.number,
                    },
                    aired: d.aired,
                    timestamp,
                    poster,
                    banner,
                    season_poster,
                    season_banner,
                    backdrop,
                }))
            }
            api::PendingKind::Movie { movie } => {
                let Some((timestamp,)) =
                    s.pending_timestamp_for_movie.bind((user, movie))?.first()?
                else {
                    return Ok(None);
                };

                let Some(d) = s.pending_movie_detail.bind((user, movie))?.first()? else {
                    return Ok(None);
                };

                let cfg = s.user_language(user)?;
                let poster = s.image.image_for_movie(movie, ImageKind::Poster)?;
                let banner = s.image.image_for_movie(movie, ImageKind::Banner)?;
                let backdrop = s.image.image_for_movie(movie, ImageKind::Backdrop)?;

                let title = s
                    .translations
                    .movie(user, movie, cfg)?
                    .title()
                    .map(str::to_owned);

                Ok(Some(api::Pending {
                    info: api::PendingInfo::Movie { movie, title },
                    aired: d.release_date,
                    timestamp,
                    poster,
                    banner,
                    season_poster: None,
                    season_banner: None,
                    backdrop,
                }))
            }
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn schedule(
        &self,
        user: UserId,
        start_offset_days: i32,
        days: u32,
        time: api::TimeInfo,
    ) -> Result<Vec<api::ScheduledDay>> {
        type DayShows = Vec<(
            ShowId,
            String,
            Option<api::Image>,
            Option<api::Image>,
            Vec<api::ScheduleEpisode>,
        )>;

        let today = time.now().date(time.clone());

        let start = if start_offset_days >= 0 {
            today.checked_add_days(start_offset_days as u32)
        } else {
            today.checked_sub_days(start_offset_days.unsigned_abs())
        };

        let Some(start) = start else {
            return Ok(vec![]);
        };

        let Some(end) = start.checked_add_days(days) else {
            return Ok(vec![]);
        };

        let start = start.to_timestamp_at_midnight_zoned(time.tz().clone())?;
        let end = end.to_timestamp_at_midnight_zoned(time.tz().clone())?;

        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut days_map = Vec::<(Date, DayShows, Vec<api::ScheduleMovie>)>::new();

            let s = &mut *s;

            let config = s.user_language(user)?;

            // Resolved show titles, cached so each show is looked up once.
            let mut show_titles: HashMap<ShowId, String> = HashMap::new();

            // Resolved (backdrop, poster) per show, cached so each show is
            // looked up once. Drives the hover background and schedule poster.
            let mut show_images: HashMap<ShowId, (Option<api::Image>, Option<api::Image>)> =
                HashMap::new();

            let mut stmt = s.list_schedule.bind((user, start, end))?;

            while let Some(r) = stmt.next()? {
                let Some(day) = r.aired else { continue };

                let ep = api::ScheduleEpisode {
                    season: r.season,
                    episode: r.number,
                    aired: day,
                };

                let day = day.date(time.clone());

                let show_title = match show_titles.get(&r.show_id) {
                    Some(title) => title.clone(),
                    None => {
                        let title = s
                            .translations
                            .show(user, r.show_id, config)?
                            .title()
                            .unwrap_or_default()
                            .to_owned();

                        show_titles.insert(r.show_id, title.clone());
                        title
                    }
                };

                let (backdrop, poster) = match show_images.get(&r.show_id) {
                    Some(images) => images.clone(),
                    None => {
                        let backdrop = s.image.image_for_show(r.show_id, ImageKind::Backdrop)?;
                        let poster = s.image.image_for_show(r.show_id, ImageKind::Poster)?;
                        show_images.insert(r.show_id, (backdrop.clone(), poster.clone()));
                        (backdrop, poster)
                    }
                };

                if let Some(day_entry) = days_map.iter_mut().find(|(d, ..)| d == &day) {
                    if let Some(show_entry) =
                        day_entry.1.iter_mut().find(|(id, ..)| *id == r.show_id)
                    {
                        show_entry.4.push(ep);
                    } else {
                        day_entry
                            .1
                            .push((r.show_id, show_title, backdrop, poster, vec![ep]));
                    }
                } else {
                    days_map.push((
                        day,
                        vec![(r.show_id, show_title, backdrop, poster, vec![ep])],
                        Vec::new(),
                    ));
                }
            }

            stmt.reset()?;

            let mut stmt = s.list_schedule_movies.bind((user, start, end))?;

            while let Some(r) = stmt.next()? {
                let Some(released) = r.released else { continue };

                let title = s
                    .translations
                    .movie(user, r.movie_id, config)?
                    .title()
                    .unwrap_or_default()
                    .to_owned();

                let backdrop = s.image.image_for_movie(r.movie_id, ImageKind::Backdrop)?;
                let poster = s.image.image_for_movie(r.movie_id, ImageKind::Poster)?;

                let movie = api::ScheduleMovie {
                    movie_id: r.movie_id,
                    title,
                    released,
                    backdrop,
                    poster,
                };

                let day = released.date(time.clone());

                if let Some(day_entry) = days_map.iter_mut().find(|(d, ..)| d == &day) {
                    day_entry.2.push(movie);
                } else {
                    days_map.push((day, Vec::new(), vec![movie]));
                }
            }

            stmt.reset()?;

            // Movie-only days may be appended out of order; sort so the frontend can rely
            // on the last day being the furthest date when extending the calendar grid.
            days_map.sort_by_key(|(a, ..)| *a);

            let out = days_map
                .into_iter()
                .map(|(date, show, movies)| api::ScheduledDay {
                    date,
                    shows: show
                        .into_iter()
                        .map(|(show_id, show_title, backdrop, poster, episodes)| {
                            api::ScheduledEntry {
                                show_id,
                                show_title,
                                episodes,
                                backdrop,
                                poster,
                            }
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
        spawn_blocking(move || s.load_config()).await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn save_config(&self, config: &Config) -> Result<()> {
        let config = config.clone();

        let result = self.transaction(move |s| {
            s.set_config("tvdb_api_key", config.tvdb_api_key)?;

            if let Some(ref pin) = config.tvdb_pin {
                s.set_config("tvdb_pin", pin)?;
            } else {
                s.delete_config("tvdb_pin")?;
            }

            s.set_config("tmdb_api_key", config.tmdb_api_key)?;

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
                config.auto_sync_interval_hours.to_string(),
            )?;

            s.set_config("page_title", &config.page_title)?;
            s.set_config(
                "release_filters",
                config::encode_filter_rules(&config.release_filters),
            )?;
            s.set_config(
                "air_date_filters",
                config::encode_filter_rules(&config.air_date_filters),
            )?;
            s.set_config("sync_kinds", config::encode_sync_kinds(&config.sync_kinds))?;
            s.set_config(
                "sync_languages",
                config::encode_sync_languages(&config.sync_languages),
            )?;

            let bool_text = |v: bool| if v { "true" } else { "false" };
            let access = &config.cloudflare_access;
            s.set_config("cloudflare_access_enabled", bool_text(access.enabled))?;
            s.set_config("cloudflare_team_domain", &access.team_domain)?;
            s.set_config("cloudflare_audience", &access.audience)?;
            s.set_config(
                "cloudflare_trust_email_header",
                bool_text(access.trust_email_header),
            )?;
            s.set_config("cloudflare_verify_jwt", bool_text(access.verify_jwt))?;
            Ok(())
        });

        result.await
    }

    /// The locales users who track the show, or picked a language for it, see
    /// it in; [`api::Locale::DEFAULT`] (its original language) is left out.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn show_viewer_languages(&self, show_id: ShowId) -> Result<Vec<api::Locale>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut stmt = s.show_viewer_languages.bind((show_id,))?;
            let mut rows = Vec::new();

            while let Some(row) = stmt.next()? {
                rows.push(row);
            }

            stmt.reset()?;
            Ok(viewer_languages(rows))
        });

        result.await?
    }

    /// As [`Self::show_viewer_languages`], for a movie.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn movie_viewer_languages(
        &self,
        movie_id: MovieId,
    ) -> Result<Vec<api::Locale>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut stmt = s.movie_viewer_languages.bind((movie_id,))?;
            let mut rows = Vec::new();

            while let Some(row) = stmt.next()? {
                rows.push(row);
            }

            stmt.reset()?;
            Ok(viewer_languages(rows))
        });

        result.await?
    }

    /// The user's preferences: the default for every key without a row.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn load_preferences(&self, user: UserId) -> Result<api::Preferences> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut preferences = api::Preferences::default();
            let mut stmt = s.list_user_config.bind((user,))?;

            while let Some((key, value)) = stmt.next()? {
                if let Err(error) = preferences.decode(&key, &value) {
                    tracing::warn!(%user, key, value, %error, "Skipping stored preference");
                }
            }

            stmt.reset()?;
            Ok(preferences)
        });

        result.await?
    }

    /// Store the user's preferences, keeping a row only for each value that
    /// differs from the default.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn save_preferences(
        &self,
        user: UserId,
        preferences: &api::Preferences,
    ) -> Result<()> {
        let rows = preferences.encode();

        let result = self.transaction(move |s| {
            s.clear_user_config.execute((user,))?;

            for (key, value) in rows {
                s.insert_user_config.execute((user, key, value))?;
            }

            Ok(())
        });

        result.await
    }

    /// The most-used per-show/per-movie custom language overrides, ordered
    /// most-used first, as recomputed by the periodic task.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn get_state_top_languages(&self) -> Result<Vec<api::Locale>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let raw = s.get_state_top_languages.query()?.next()?;

            Ok(raw
                .as_deref()
                .and_then(|v| serde_json::from_str::<Vec<api::Locale>>(v).ok())
                .unwrap_or_default())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_state_top_languages(&self, languages: Vec<api::Locale>) -> Result<()> {
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
    pub(crate) async fn compute_top_languages(&self, n: usize) -> Result<Vec<api::Locale>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut counts: HashMap<api::Locale, usize> = HashMap::new();

            let mut tally = |language: api::Locale| {
                if !language.is_default() {
                    *counts.entry(language).or_default() += 1;
                }
            };

            let mut stmt = s.list_show_languages.query()?;

            while let Some(r) = stmt.next()? {
                tally(r.language.0);
            }

            stmt.reset()?;

            let mut stmt = s.list_movie_languages.query()?;

            while let Some(r) = stmt.next()? {
                tally(r.language.0);
            }

            stmt.reset()?;

            let mut ranked: Vec<(api::Locale, usize)> = counts.into_iter().collect();
            // Most-used first; break ties by code for a stable result.
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

            Ok(ranked.into_iter().take(n).map(|(lang, _)| lang).collect())
        });

        result.await?
    }
}

/// Each viewer's media language, else their global one, without duplicates or
/// the default.
fn viewer_languages(rows: Vec<(Pref<api::Locale>, Pref<api::Locale>)>) -> Vec<api::Locale> {
    let mut out = Vec::new();

    for (Pref(media), Pref(global)) in rows {
        let language = media.or(global);

        if !language.language().is_default() && !out.contains(&language) {
            out.push(language);
        }
    }

    out
}

fn cutoff_timestamp(interval_hours: u32) -> Timestamp {
    let hours = interval_hours.max(1);
    let inner = Timestamp::now().inner();
    let ts = inner
        .checked_sub(jiff::Span::new().hours(hours))
        .unwrap_or(inner);
    Timestamp::from_jiff(ts)
}

fn show_from_row(row: ShowRow, strings: api::Translations) -> api::Show {
    api::Show {
        id: row.id,
        strings,
        first_air_date: row.first_air,
        tracked: row.tracked,
        auto_sync: row.auto_sync,
        remotes: Vec::new(),
        images: Vec::new(),
        poster: None,
        banner: None,
        backdrop: None,
        user_selected: Vec::new(),
        last_synced_at: row.last_synced_at,
        language: row.language.0,
        include_specials: row.include_specials.0,
        air_date_filters: row
            .air_date_filters
            .as_deref()
            .and_then(config::decode_filter_rules),
    }
}

/// Default merge priority for a freshly-added remote (lower wins). The global
/// `config.sync_kinds` order is the default source priority, so reordering the
/// sources in settings sets the default applied to remotes added afterward. A
/// show's own remote order (set by reordering its remotes) overrides this
/// default and is preserved. Sources absent from the global list rank after the
/// listed ones, keeping the built-in TVmaze < TMDB < TVDB < IMDb < Unknown order.
fn remote_entry(r: RemoteRow) -> api::RemoteEntry {
    api::RemoteEntry {
        id: r.id,
        slug: r.slug,
        remote: Remote::new(r.source, r.value),
        enabled: r.enabled,
        priority: r.priority,
        sync_kinds: r.sync_kinds,
        cache: parse_remote_cache(r.cache),
    }
}

fn default_remote_priority(source: RemoteSource, config: &Config) -> i32 {
    if let Some(idx) = config.sync_kinds.iter().position(|s| s.source == source) {
        return idx as i32;
    }

    let base = config.sync_kinds.len() as i32;

    base + match source {
        RemoteSource::Tvmaze => 0,
        RemoteSource::Tmdb => 1,
        RemoteSource::Tvdb => 2,
        RemoteSource::Imdb => 3,
        RemoteSource::Unknown => 4,
    }
}

fn image_from_row(row: ImageRow) -> api::MediaImage {
    api::MediaImage {
        id: row.id,
        kind: row.kind,
        source: row.source,
        image: Image::new(row.source, &row.path),
        score: row.score,
    }
}

fn show_image_from_row(row: ShowImageRow) -> api::MediaImage {
    api::MediaImage {
        id: row.id,
        kind: row.kind,
        source: row.source,
        image: Image::new(row.source, &row.path),
        score: row.score,
    }
}

fn movie_image_from_row(r: MovieImageRow) -> api::MediaImage {
    api::MediaImage {
        id: r.id,
        kind: r.kind,
        source: r.source,
        image: Image::new(r.source, &r.path),
        score: r.score,
    }
}

/// Assemble an [`api::Credit`], taking the person-name and character
/// [`Translations`] out of their maps (empty sets resolved to `locale` when
/// absent - e.g. a person not yet synced, or crew with no character).
fn credit_from_row(
    r: CreditRow,
    names: &mut HashMap<CreditId, api::Translations>,
    characters: &mut HashMap<CreditId, api::Translations>,
    locale: api::Locale,
) -> Credit {
    let profile = match (r.profile_source, r.profile_path) {
        (Some(source), Some(path)) => Some(Image::new(source, &path)),
        _ => None,
    };

    Credit {
        person_id: r.person_id,
        name: names
            .remove(&r.id)
            .unwrap_or_else(|| api::Translations::new(locale)),
        profile,
        kind: r.credit_type,
        character: characters
            .remove(&r.id)
            .unwrap_or_else(|| api::Translations::new(locale)),
        department: r.department,
        job: r.job,
        episode_count: r.episode_count,
        order: r.sort_order,
    }
}

/// Assemble an [`api::PersonCredit`] for the person detail page, building the
/// owner's title and (for cast) the character name from their raw per-locale
/// strings, resolved against `locale` (the owner's display language).
fn person_credit_from_row(
    r: PersonCreditRow,
    owner: api::CreditOwner,
    titles: &mut HashMap<CreditId, Vec<(api::Locale, String)>>,
    characters: &mut HashMap<CreditId, Vec<(api::Locale, String)>>,
    locale: api::Locale,
) -> api::PersonCredit {
    let poster = match (r.poster_source, r.poster_path) {
        (Some(source), Some(path)) => Some(Image::new(source, &path)),
        _ => None,
    };

    let mut title = api::Translations::new(locale);
    for (loc, text) in titles.remove(&r.id).unwrap_or_default() {
        title.insert(api::StringKind::Title, loc, &text);
    }

    let mut character = api::Translations::new(locale);
    for (loc, text) in characters.remove(&r.id).unwrap_or_default() {
        character.insert(api::StringKind::Character, loc, &text);
    }

    api::PersonCredit {
        owner,
        title,
        poster,
        kind: r.credit_type,
        character,
        department: r.department,
        job: r.job,
        episode_count: r.episode_count,
        order: r.sort_order,
        date: r.date,
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

    if r.user_selected {
        target.user_selected.push(r.kind);
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

    if sel.user_selected {
        target.user_selected.push(sel.kind);
    }
}

fn season_from_row(r: SeasonRow, strings: api::Translations) -> api::Season {
    api::Season {
        id: r.id,
        show_id: r.show_id,
        season: r.season,
        air_date: r.air_date,
        strings,
        poster: match (r.poster_source, r.poster_path) {
            (Some(source), Some(path)) => Some(api::Image::new(source, &path)),
            _ => None,
        },
        watched_count: r.watched_count,
        total_count: r.total_count,
    }
}

fn episode_from_row(r: EpisodeRow, strings: api::Translations) -> api::Episode {
    api::Episode {
        id: r.id,
        show_id: r.show_id,
        season: r.season,
        episode: r.number,
        absolute_number: r.absolute_number,
        strings,
        aired: r.aired,
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

fn movie_from_row(row: MovieRow, strings: api::Translations) -> api::Movie {
    api::Movie {
        id: row.id,
        strings,
        release_date: row.release_date,
        remotes: Vec::new(),
        tracked: row.tracked,
        auto_sync: row.auto_sync,
        pending: None,
        images: Vec::new(),
        poster: None,
        banner: None,
        backdrop: None,
        user_selected: Vec::new(),
        last_synced_at: row.last_synced_at,
        releases: Vec::new(),
        language: row.language.0,
        release_filters: row
            .release_filters
            .as_deref()
            .and_then(config::decode_filter_rules),
    }
}

fn media_item_from_row(
    r: MediaItemRow,
    kind: api::MediaKind,
    strings: api::Translations,
) -> api::MediaItem {
    api::MediaItem {
        id: r.id.cast_unsigned(),
        kind,
        strings,
        date: r.date,
        poster: None,
        banner: None,
        backdrop: None,
        tracked: r.tracked,
        last_watched_at: None,
        unwatched_episodes: 0,
        next_regular: false,
        next_specials: false,
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

/// Whether the database holds no tables of its own yet.
fn is_empty(c: &sqll::Connection) -> Result<bool> {
    let mut q = c.prepare(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' LIMIT 1",
    )?;
    Ok(q.next::<i64>()?.is_none())
}

/// Bring the schema up to date. An empty database is built from the base
/// schema (the first migration) alone, with every later migration recorded
/// without running; any other database applies each unrecorded migration in
/// order. Each migration runs and is recorded in one transaction, so a failed
/// one leaves the database as it was.
fn do_migrations(c: &sqll::Connection) -> Result<()> {
    let empty = is_empty(c).context("Checking whether the database is empty")?;

    let mut ids: Vec<_> = Migrations::iter().collect();
    ids.sort();

    if empty {
        // The migrations table is created in the same transaction: left behind
        // alone, it would make the database read as not empty.
        return in_transaction(c, || {
            c.execute(MIGRATIONS_INIT)
                .context("Creating migrations table")?;

            let Some((base, rest)) = ids.split_first() else {
                return Ok(());
            };

            apply(c, base.as_ref()).with_context(|| anyhow!("Migration {base}"))?;

            for id in rest {
                record(c, id.as_ref()).with_context(|| anyhow!("Recording migration {id}"))?;
            }

            Ok(())
        });
    }

    c.execute(MIGRATIONS_INIT)
        .context("Creating migrations table")?;

    let mut select = c.prepare("SELECT applied_at FROM migrations WHERE id = ?")?;

    for file in &ids {
        let id = file.as_ref();

        let result: Result<()> = (|| {
            select.reset()?;
            select.bind(id)?;

            if let Some(applied_at) = select.next::<String>()? {
                tracing::debug!(id, applied_at, "Migration already applied");
                return Ok(());
            }

            in_transaction(c, || apply(c, id))
        })();

        result.with_context(|| anyhow!("Migration {id}"))?;
    }

    Ok(())
}

/// Run `f` inside `BEGIN IMMEDIATE` / `COMMIT` on `c`, rolling back when it
/// fails.
fn in_transaction(c: &sqll::Connection, f: impl FnOnce() -> Result<()>) -> Result<()> {
    c.execute("BEGIN IMMEDIATE")?;

    let result = f().and_then(|()| Ok(c.execute("COMMIT")?));

    if result.is_err() {
        // Fails only when SQLite already rolled the transaction back itself.
        _ = c.execute("ROLLBACK");
    }

    result
}

/// Fail unless the database has every migration applied.
fn ensure_migrated(c: &sqll::Connection) -> Result<()> {
    anyhow::ensure!(
        !is_empty(c).context("Checking whether the database is empty")?,
        "The database is empty"
    );

    let mut select = c
        .prepare("SELECT 1 FROM migrations WHERE id = ?")
        .context("Reading the migrations table")?;

    let mut pending = Vec::new();

    for file in Migrations::iter() {
        select.reset()?;
        select.bind(file.as_ref())?;

        if select.next::<i64>()?.is_none() {
            pending.push(file);
        }
    }

    pending.sort();

    anyhow::ensure!(
        pending.is_empty(),
        "The database has pending migrations ({}); start the current server against it once to migrate it",
        pending.join(", ")
    );

    Ok(())
}

/// Run one migration and record it.
fn apply(c: &sqll::Connection, id: &str) -> Result<()> {
    let asset = Migrations::get(id).with_context(|| anyhow!("Migration file not found: {id}"))?;

    let sql = str::from_utf8(asset.data.as_ref())
        .with_context(|| anyhow!("Migration {id} is not valid UTF-8"))?;

    c.execute(sql)
        .with_context(|| anyhow!("Executing migration {id}"))?;

    record(c, id)?;

    tracing::info!(id, "Migration applied");
    Ok(())
}

/// Record a migration as applied without running it.
fn record(c: &sqll::Connection, id: &str) -> Result<()> {
    let now = Timestamp::now().to_string();

    let mut insert = c.prepare("INSERT INTO migrations (id, applied_at) VALUES (?, ?)")?;

    insert
        .execute((id, now.as_str()))
        .with_context(|| anyhow!("Updating migrations table {id}"))?;

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
        OpenMode::ReadOnly => {
            c.execute("PRAGMA busy_timeout = 5000;")?;
        }
    }

    Ok(())
}
