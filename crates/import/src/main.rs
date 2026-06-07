use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use chrono::NaiveDate;
use clap::Parser;
use serde::Deserialize;
use uuid::Uuid;

// ── Minimal YAML-compatible types mirroring the ontv model ───────────────────

#[derive(Debug, Deserialize)]
struct YamlSeries {
    id: Uuid,
    title: String,
    #[serde(default)]
    first_air_date: Option<NaiveDate>,
    #[serde(default)]
    overview: String,
    #[serde(default)]
    graphics: YamlSeriesGraphics,
    #[serde(default)]
    tracked: bool,
    #[serde(default)]
    remote_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct YamlSeriesGraphics {
    #[serde(default)]
    poster: Option<String>,
    #[serde(default)]
    banner: Option<String>,
    #[serde(default)]
    fanart: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YamlMovie {
    id: Uuid,
    title: String,
    #[serde(default)]
    release_date: Option<NaiveDate>,
    #[serde(default)]
    overview: String,
    #[serde(default)]
    graphics: YamlMovieGraphics,
    #[serde(default)]
    remote_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct YamlMovieGraphics {
    #[serde(default)]
    poster: Option<String>,
    #[serde(default)]
    banner: Option<String>,
    #[serde(default)]
    fanart: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YamlSeason {
    #[serde(default)]
    number: YamlSeasonNumber,
    #[serde(default)]
    air_date: Option<NaiveDate>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    overview: String,
    #[serde(default)]
    graphics: YamlSeasonGraphics,
}

#[derive(Debug, Default, Deserialize)]
struct YamlSeasonGraphics {
    #[serde(default)]
    poster: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YamlEpisode {
    id: Uuid,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    overview: String,
    #[serde(default)]
    absolute_number: Option<u32>,
    #[serde(default)]
    season: YamlSeasonNumber,
    number: u32,
    #[serde(default)]
    aired: Option<NaiveDate>,
    #[serde(default)]
    graphics: YamlEpisodeGraphics,
    #[serde(default)]
    remote_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct YamlEpisodeGraphics {
    #[serde(default)]
    filename: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum YamlSeasonNumber {
    #[default]
    Specials,
    Number(u32),
}

impl From<YamlSeasonNumber> for api::SeasonNumber {
    fn from(n: YamlSeasonNumber) -> Self {
        match n {
            YamlSeasonNumber::Specials => api::SeasonNumber::Specials,
            YamlSeasonNumber::Number(n) => api::SeasonNumber::Number(n),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum YamlWatched {
    Episode {
        #[allow(dead_code)]
        id: Uuid,
        timestamp: chrono::DateTime<chrono::Utc>,
        series: Uuid,
        episode: Uuid,
    },
    Movie {
        #[allow(dead_code)]
        id: Uuid,
        timestamp: chrono::DateTime<chrono::Utc>,
        movie: Uuid,
    },
}

#[derive(Debug, Deserialize)]
struct YamlConfig {
    #[serde(default)]
    theme: String,
    #[serde(default)]
    tvdb_legacy_apikey: String,
    #[serde(default)]
    tmdb_api_key: String,
    #[serde(default = "default_days")]
    schedule_duration_days: u32,
    #[serde(default = "default_limit")]
    dashboard_limit: u32,
    #[serde(default = "default_page")]
    dashboard_page: u32,
}

fn default_days() -> u32 {
    7
}
fn default_limit() -> u32 {
    1
}
fn default_page() -> u32 {
    6
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(about = "Import ontv YAML data into ontv-musli-web SQLite database")]
struct Args {
    /// Path to the ontv config directory (contains series.yaml, movies.yaml, etc.)
    #[arg(long, default_value = "~/.config/ontv")]
    source: String,

    /// Path to the output SQLite database.
    #[arg(long, default_value = "ontv.db")]
    db: PathBuf,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs_home()
    {
        return home.join(rest);
    }

    PathBuf::from(path)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn image(s: Option<&String>) -> Option<api::Image> {
    let s = s?;
    if s.is_empty() {
        return None;
    }
    Some(api::Image::from_raw(s.as_str()))
}

fn image_source(img: &api::Image) -> api::ImageSource {
    match img.source() {
        "tvdb" => api::ImageSource::Tvdb,
        "tmdb" => api::ImageSource::Tmdb,
        _ => api::ImageSource::Local,
    }
}

async fn import_series_images(
    db: &db::Database,
    series_id: api::SeriesId,
    g: &YamlSeriesGraphics,
) -> Result<()> {
    if let Some(img) = image(g.poster.as_ref()) {
        db.upsert_series_image(series_id, api::ImageKind::Poster, image_source(&img), img.path())
            .await?;
    }
    if let Some(img) = image(g.banner.as_ref()) {
        db.upsert_series_image(series_id, api::ImageKind::Banner, image_source(&img), img.path())
            .await?;
    }
    if let Some(img) = image(g.fanart.as_ref()) {
        db.upsert_series_image(series_id, api::ImageKind::Fanart, image_source(&img), img.path())
            .await?;
    }
    Ok(())
}

async fn import_movie_images(
    db: &db::Database,
    movie_id: api::MovieId,
    g: &YamlMovieGraphics,
) -> Result<()> {
    if let Some(img) = image(g.poster.as_ref()) {
        db.upsert_movie_image(movie_id, api::ImageKind::Poster, image_source(&img), img.path())
            .await?;
    }
    if let Some(img) = image(g.banner.as_ref()) {
        db.upsert_movie_image(movie_id, api::ImageKind::Banner, image_source(&img), img.path())
            .await?;
    }
    if let Some(img) = image(g.fanart.as_ref()) {
        db.upsert_movie_image(movie_id, api::ImageKind::Fanart, image_source(&img), img.path())
            .await?;
    }
    Ok(())
}

fn remote_id(s: Option<&String>) -> Option<api::RemoteId> {
    let s = s?;
    if s.is_empty() {
        return None;
    }
    Some(api::RemoteId::from_raw(s.as_str()))
}

#[derive(Debug, Deserialize)]
struct YamlRemote {
    #[serde(rename = "type")]
    kind: String,
    uuid: Uuid,
    #[serde(default)]
    remotes: Vec<String>,
}

fn naive_to_date(d: NaiveDate) -> api::Date {
    d.to_string()
        .parse()
        .expect("NaiveDate always formats as valid ISO date")
}

fn chrono_to_timestamp(dt: chrono::DateTime<chrono::Utc>) -> api::Timestamp {
    dt.to_rfc3339()
        .parse::<api::Timestamp>()
        .unwrap_or_else(|_| api::Timestamp::now())
}

fn parse_yaml_docs<T>(path: &Path) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

    let mut out = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(&content) {
        let value = T::deserialize(doc)
            .with_context(|| format!("parsing document in {}", path.display()))?;
        out.push(value);
    }
    Ok(out)
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?),
        )
        .init();

    let args = Args::parse();
    let source = expand_tilde(&args.source);

    tracing::info!("opening database at {}", args.db.display());
    let db = db::Database::open(&args.db, db::OpenMode::Bulk).context("opening database")?;

    // Maps from old UUID → new SQLite rowid
    let mut series_map: HashMap<Uuid, api::SeriesId> = HashMap::new();
    let mut episode_map: HashMap<Uuid, api::EpisodeId> = HashMap::new();
    let mut movie_map: HashMap<Uuid, api::MovieId> = HashMap::new();

    // Dedup maps keyed by remote_id for series/movies, (id, timestamp) for watched
    let mut series_by_remote: HashMap<String, api::SeriesId> = db
        .series()
        .await
        .context("loading existing series")?
        .into_iter()
        .flat_map(|s| {
            let id = s.id;
            s.remotes.into_iter().map(move |r| (r.as_str().to_owned(), id))
        })
        .collect();

    let mut movies_by_remote: HashMap<String, api::MovieId> = db
        .movies()
        .await
        .context("loading existing movies")?
        .into_iter()
        .flat_map(|m| {
            let id = m.id;
            m.remotes.into_iter().map(move |r| (r.as_str().to_owned(), id))
        })
        .collect();

    // (episode_id or movie_id as u64, timestamp string) — covers both kinds
    let mut watched_seen: std::collections::HashSet<(u64, String)> = db
        .all_watched()
        .await
        .context("loading existing watched")?
        .into_iter()
        .map(|w| {
            let item_id = match w.kind {
                api::WatchedKind::Episode { episode, .. } => episode.get(),
                api::WatchedKind::Movie { movie } => movie.get(),
            };
            (item_id, w.timestamp.to_string())
        })
        .collect();

    // ── Config ────────────────────────────────────────────────────────────────
    let config_path = source.join("config.yaml");
    if config_path.exists() {
        tracing::info!("importing config");
        let cfg: YamlConfig = serde_yaml::from_str(
            &std::fs::read_to_string(&config_path).context("reading config.yaml")?,
        )
        .context("parsing config.yaml")?;

        let theme = match cfg.theme.as_str() {
            "light" => api::ThemeType::Light,
            _ => api::ThemeType::Dark,
        };

        db.save_config(&api::Config {
            theme,
            tvdb_legacy_apikey: cfg.tvdb_legacy_apikey,
            tmdb_api_key: cfg.tmdb_api_key,
            schedule_duration_days: cfg.schedule_duration_days,
            dashboard_limit: cfg.dashboard_limit,
            dashboard_page: cfg.dashboard_page,
            auto_sync_enabled: false,
            auto_sync_interval_hours: 24,
        })
        .await
        .context("saving config")?;
    }

    // ── Series ────────────────────────────────────────────────────────────────
    let series_path = source.join("series.yaml");
    let all_series: Vec<YamlSeries> =
        parse_yaml_docs(&series_path).context("parsing series.yaml")?;

    let total_series = all_series.len();
    tracing::info!("importing {total_series} series");

    for (i, s) in all_series.iter().enumerate() {
        let series_id = if let Some(rid) = &s.remote_id
            && let Some(&existing_id) = series_by_remote.get(rid.as_str())
        {
            existing_id
        } else {
            let inserted = db
                .create_series(
                    &s.title,
                    s.first_air_date
                        .as_ref()
                        .map(|d| naive_to_date(*d))
                        .as_ref(),
                    &s.overview,
                )
                .await
                .with_context(|| format!("inserting series '{}'", s.title))?;

            if !s.tracked {
                db.set_series_tracked(inserted.id, false).await?;
            }
            import_series_images(&db, inserted.id, &s.graphics).await?;

            if let Some(rid) = &s.remote_id {
                let remote = api::RemoteId::from_raw(rid.as_str());
                db.add_series_remote(inserted.id, &remote).await?;
                series_by_remote.insert(rid.clone(), inserted.id);
            }

            inserted.id
        };

        series_map.insert(s.id, series_id);

        if (i + 1) % 50 == 0 || i + 1 == total_series {
            tracing::info!("  series {}/{total_series}", i + 1);
        }
    }

    // ── Seasons + Episodes ────────────────────────────────────────────────────
    tracing::info!("importing seasons and episodes for {total_series} series");

    for (i, s) in all_series.iter().enumerate() {
        let series_id = series_map[&s.id];

        let seasons_file = source.join("seasons").join(format!("{}.yaml", s.id));
        if seasons_file.exists() {
            let seasons: Vec<YamlSeason> = parse_yaml_docs(&seasons_file)
                .with_context(|| format!("parsing seasons for {}", s.id))?;

            for season in seasons {
                db.upsert_season(
                    series_id,
                    season.number.into(),
                    season.air_date.as_ref().map(|d| naive_to_date(*d)).as_ref(),
                    season.name.as_deref(),
                    &season.overview,
                    image(season.graphics.poster.as_ref()).as_ref(),
                )
                .await
                .with_context(|| format!("inserting season for series {}", s.id))?;
            }
        }

        let episodes_file = source.join("episodes").join(format!("{}.yaml", s.id));
        if episodes_file.exists() {
            let episodes: Vec<YamlEpisode> = parse_yaml_docs(&episodes_file)
                .with_context(|| format!("parsing episodes for {}", s.id))?;

            for ep in episodes {
                let inserted = db
                    .upsert_episode(
                        series_id,
                        ep.season.into(),
                        ep.number,
                        ep.absolute_number,
                        ep.name.as_deref(),
                        &ep.overview,
                        ep.aired.as_ref().map(|d| naive_to_date(*d)).as_ref(),
                        image(ep.graphics.filename.as_ref()).as_ref(),
                        remote_id(ep.remote_id.as_ref()).as_ref(),
                    )
                    .await
                    .with_context(|| {
                        format!("inserting episode {} for series {}", ep.number, s.id)
                    })?;

                episode_map.insert(ep.id, inserted.id);
            }
        }

        if (i + 1) % 50 == 0 || i + 1 == total_series {
            tracing::info!(
                "  seasons/episodes {}/{total_series} series ({} episodes so far)",
                i + 1,
                episode_map.len()
            );
        }
    }

    tracing::info!("imported {} episodes total", episode_map.len());

    // ── Movies ────────────────────────────────────────────────────────────────
    let movies_path = source.join("movies.yaml");
    let all_movies: Vec<YamlMovie> =
        parse_yaml_docs(&movies_path).context("parsing movies.yaml")?;

    let total_movies = all_movies.len();
    tracing::info!("importing {total_movies} movies");

    for (i, m) in all_movies.iter().enumerate() {
        let movie_id = if let Some(rid) = &m.remote_id
            && let Some(&existing_id) = movies_by_remote.get(rid.as_str())
        {
            existing_id
        } else {
            let inserted = db
                .create_movie(
                    &m.title,
                    m.release_date.as_ref().map(|d| naive_to_date(*d)).as_ref(),
                    &m.overview,
                )
                .await
                .with_context(|| format!("inserting movie '{}'", m.title))?;

            import_movie_images(&db, inserted.id, &m.graphics).await?;

            if let Some(rid) = &m.remote_id {
                let remote = api::RemoteId::from_raw(rid.as_str());
                db.add_movie_remote(inserted.id, &remote).await?;
                movies_by_remote.insert(rid.clone(), inserted.id);
            }

            inserted.id
        };

        movie_map.insert(m.id, movie_id);

        if (i + 1) % 20 == 0 || i + 1 == total_movies {
            tracing::info!("  movies {}/{total_movies}", i + 1);
        }
    }

    // ── Watched ───────────────────────────────────────────────────────────────
    let watched_path = source.join("watched.yaml");
    let all_watched: Vec<YamlWatched> =
        parse_yaml_docs(&watched_path).context("parsing watched.yaml")?;

    let total_watched = all_watched.len();
    tracing::info!("importing {total_watched} watched entries");

    let mut skipped = 0usize;

    for (i, w) in all_watched.into_iter().enumerate() {
        match w {
            YamlWatched::Episode {
                timestamp,
                series,
                episode,
                ..
            } => {
                let Some(&series_id) = series_map.get(&series) else {
                    skipped += 1;
                    continue;
                };
                let Some(&episode_id) = episode_map.get(&episode) else {
                    skipped += 1;
                    continue;
                };
                let ts = chrono_to_timestamp(timestamp);
                let key = (episode_id.get(), ts.to_string());
                if watched_seen.contains(&key) {
                    skipped += 1;
                    continue;
                }
                db.mark_watched(
                    api::WatchedKind::Episode {
                        series: series_id,
                        episode: episode_id,
                    },
                    ts,
                )
                .await
                .context("inserting watched episode")?;
                watched_seen.insert(key);
            }
            YamlWatched::Movie {
                timestamp, movie, ..
            } => {
                let Some(&movie_id) = movie_map.get(&movie) else {
                    skipped += 1;
                    continue;
                };
                let ts = chrono_to_timestamp(timestamp);
                let key = (movie_id.get(), ts.to_string());
                if watched_seen.contains(&key) {
                    skipped += 1;
                    continue;
                }
                db.mark_watched(api::WatchedKind::Movie { movie: movie_id }, ts)
                    .await
                    .context("inserting watched movie")?;
                watched_seen.insert(key);
            }
        }

        if (i + 1) % 1000 == 0 || i + 1 == total_watched {
            tracing::info!("  watched {}/{total_watched}", i + 1);
        }
    }

    if skipped > 0 {
        tracing::warn!(
            "{skipped} watched entries skipped (referencing missing series/episodes/movies)"
        );
    }

    // ── Remotes ───────────────────────────────────────────────────────────────
    let remotes_path = source.join("remotes.yaml");
    if remotes_path.exists() {
        let all_remotes: Vec<YamlRemote> =
            parse_yaml_docs(&remotes_path).context("parsing remotes.yaml")?;

        tracing::info!("importing remotes from {} entries", all_remotes.len());
        let mut added = 0usize;

        for entry in &all_remotes {
            match entry.kind.as_str() {
                "series" => {
                    let Some(&series_id) = series_map.get(&entry.uuid) else {
                        continue;
                    };
                    for rid in &entry.remotes {
                        let remote = api::RemoteId::from_raw(rid.as_str());
                        db.add_series_remote(series_id, &remote)
                            .await
                            .with_context(|| {
                                format!("adding remote {rid} to series {:?}", entry.uuid)
                            })?;
                        added += 1;
                    }
                }
                "movie" => {
                    let Some(&movie_id) = movie_map.get(&entry.uuid) else {
                        continue;
                    };
                    for rid in &entry.remotes {
                        let remote = api::RemoteId::from_raw(rid.as_str());
                        db.add_movie_remote(movie_id, &remote)
                            .await
                            .with_context(|| {
                                format!("adding remote {rid} to movie {:?}", entry.uuid)
                            })?;
                        added += 1;
                    }
                }
                _ => {}
            }
        }

        tracing::info!("added {added} remote IDs");
    }

    tracing::info!("import complete");
    Ok(())
}
