use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use chrono::NaiveDate;
use clap::Parser;
use serde::Deserialize;
use tracing::Level;
use uuid::Uuid;

use crate::db::{Database, OpenMode};

fn uuid_to_u64(uuid: Uuid) -> u64 {
    let n = uuid.as_u128();
    ((n >> 64) as u64) ^ (n as u64)
}

#[derive(Debug, Deserialize)]
struct YamlShow {
    id: Uuid,
    title: String,
    #[serde(default)]
    first_air_date: Option<NaiveDate>,
    #[serde(default)]
    overview: String,
    #[serde(default)]
    graphics: YamlShowGraphics,
    #[serde(default)]
    tracked: bool,
    #[serde(default)]
    remote_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct YamlShowGraphics {
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
    overview: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YamlEpisode {
    id: Uuid,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    absolute_number: Option<u32>,
    #[serde(default)]
    season: YamlSeasonNumber,
    number: u32,
    #[serde(default)]
    aired: Option<NaiveDate>,
    #[serde(default)]
    remote_id: Option<String>,
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
            YamlSeasonNumber::Number(n) => api::SeasonNumber::from_ordinal(n),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum YamlWatched {
    Episode {
        id: Uuid,
        timestamp: chrono::DateTime<chrono::Utc>,
        series: Uuid,
        place: String,
    },
    Movie {
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
    tvdb_api_key: String,
    #[serde(default)]
    tvdb_pin: Option<String>,
    #[serde(default)]
    tmdb_api_key: String,
    #[serde(default = "default_days")]
    schedule_duration_days: u32,
    #[serde(default = "default_page")]
    dashboard_page: u32,
}

fn default_days() -> u32 {
    7
}

fn default_page() -> u32 {
    5
}

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

async fn import_show_image(
    db: &Database,
    show_id: api::ShowId,
    kind: api::ImageKind,
    img: &api::Image,
) -> Result<()> {
    let id = api::ImageId::random();
    db.upsert_show_image(id, show_id, kind, 0, img).await?;
    db.set_show_image_selection(show_id, kind, id).await?;
    Ok(())
}

async fn import_show_images(
    db: &Database,
    show_id: api::ShowId,
    g: &YamlShowGraphics,
) -> Result<()> {
    if let Some(img) = image(g.poster.as_ref()) {
        import_show_image(db, show_id, api::ImageKind::Poster, &img).await?;
    }

    if let Some(img) = image(g.banner.as_ref()) {
        import_show_image(db, show_id, api::ImageKind::Banner, &img).await?;
    }

    if let Some(img) = image(g.fanart.as_ref()) {
        import_show_image(db, show_id, api::ImageKind::Backdrop, &img).await?;
    }

    Ok(())
}

async fn import_movie_image(
    db: &Database,
    movie_id: api::MovieId,
    kind: api::ImageKind,
    img: &api::Image,
) -> Result<()> {
    let id = api::ImageId::random();
    db.upsert_movie_image(id, movie_id, kind, 0, img).await?;
    db.set_movie_image_selection(movie_id, kind, id).await?;
    Ok(())
}

async fn import_movie_images(
    db: &Database,
    movie_id: api::MovieId,
    g: &YamlMovieGraphics,
) -> Result<()> {
    if let Some(img) = image(g.poster.as_ref()) {
        import_movie_image(db, movie_id, api::ImageKind::Poster, &img).await?;
    }

    if let Some(img) = image(g.banner.as_ref()) {
        import_movie_image(db, movie_id, api::ImageKind::Banner, &img).await?;
    }

    if let Some(img) = image(g.fanart.as_ref()) {
        import_movie_image(db, movie_id, api::ImageKind::Backdrop, &img).await?;
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

fn parse_place(s: &str) -> Option<(api::SeasonNumber, u32)> {
    let (season_str, ep_str) = s.split_once('x')?;
    let ep: u32 = ep_str.parse().ok()?;
    let season = if season_str.eq_ignore_ascii_case("s") {
        api::SeasonNumber::Specials
    } else {
        api::SeasonNumber::Number(season_str.parse().ok()?)
    };
    Some((season, ep))
}

fn naive_to_date(d: NaiveDate) -> api::Date {
    d.to_string()
        .parse()
        .expect("NaiveDate always formats as valid ISO date")
}

fn chrono_to_timestamp(dt: chrono::DateTime<chrono::Utc>) -> Result<api::Timestamp> {
    Ok(dt.to_rfc3339().parse::<api::Timestamp>()?)
}

fn parse_yaml_docs<T>(path: &Path) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    let content =
        std::fs::read_to_string(path).with_context(|| anyhow!("Reading {}", path.display()))?;

    let mut out = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(&content) {
        let value = T::deserialize(doc)
            .with_context(|| anyhow!("Parsing document in {}", path.display()))?;
        out.push(value);
    }
    Ok(out)
}

#[derive(Parser)]
#[command(about = "Import ontv YAML data into ontv-musli-web SQLite database")]
struct Args {
    /// Path to the ontv config directory (contains series.yaml, movies.yaml,
    /// etc.)
    #[arg(long, default_value = "~/.config/ontv")]
    source: String,

    /// Path to the output SQLite database.
    #[arg(long, default_value = "track.db")]
    db: PathBuf,

    /// Add logging directives.
    #[arg(long)]
    log: Vec<String>,
}

pub async fn import() -> Result<()> {
    let args = Args::parse();

    let mut filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();

    for directive in &args.log {
        filter = filter.add_directive(directive.parse()?);
    }

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let source = expand_tilde(&args.source);

    tracing::info!("Opening database at {}", args.db.display());

    let db = Database::open(&args.db, OpenMode::Bulk, 1)
        .with_context(|| anyhow!("Opening database at {}", args.db.display()))?;

    // Maps from old UUID → new SQLite rowid
    let mut show_by_uuid: HashMap<Uuid, api::ShowId> = HashMap::new();

    // Dedup maps keyed by remote_id for show/movies, (id, timestamp) for watched
    let mut show_by_remote: HashMap<String, api::ShowId> = db
        .shows()
        .await
        .context("Loading existing show")?
        .into_iter()
        .flat_map(|s| {
            let id = s.id;
            s.remotes.into_iter().map(move |r| (r.to_string(), id))
        })
        .collect();

    let mut movies_by_remote: HashMap<String, api::MovieId> = db
        .movies()
        .await
        .context("Loading existing movies")?
        .into_iter()
        .flat_map(|m| {
            let id = m.id;
            m.remotes.into_iter().map(move |r| (r.to_string(), id))
        })
        .collect();

    let config_path = source.join("config.yaml");

    if config_path.exists() {
        tracing::info!("Importing config");
        let config: YamlConfig = serde_yaml::from_str(
            &std::fs::read_to_string(&config_path).context("Reading config.yaml")?,
        )
        .context("Parsing config.yaml")?;

        let theme = match config.theme.as_str() {
            "light" => api::ThemeType::Light,
            _ => api::ThemeType::Dark,
        };

        db.save_config(&api::Config {
            theme,
            tvdb_api_key: config.tvdb_api_key,
            tvdb_pin: config.tvdb_pin,
            tmdb_api_key: config.tmdb_api_key,
            schedule_duration_days: config.schedule_duration_days,
            dashboard_page: config.dashboard_page,
            auto_sync_enabled: false,
            auto_sync_interval_hours: 24,
            timezone: String::new(),
            language: None,
            include_specials: false,
        })
        .await
        .context("Saving config")?;
    }

    let show_path = source.join("series.yaml");
    let all_shows: Vec<YamlShow> = parse_yaml_docs(&show_path).context("Parsing series.yaml")?;

    let total_show = all_shows.len();
    tracing::info!("Importing {total_show} show");

    for (i, s) in all_shows.iter().enumerate() {
        let show_id = if let Some(ref remote_id) = s.remote_id
            && let Some(&existing_id) = show_by_remote.get(remote_id)
        {
            existing_id
        } else {
            let first_air = s
                .first_air_date
                .as_ref()
                .and_then(|d| naive_to_date(*d).to_timestamp_at_midnight_utc().ok());

            let show_id = api::ShowId::new(uuid_to_u64(s.id));

            db.create_show(show_id, &s.title, first_air, &s.overview)
                .await
                .with_context(|| anyhow!("Inserting show '{}'", s.title))?;

            if !s.tracked {
                db.set_show_tracked(show_id, false).await?;
            }
            import_show_images(&db, show_id, &s.graphics).await?;

            if let Some(remote_id) = &s.remote_id {
                let remote = api::RemoteId::from_raw(remote_id);
                db.add_show_remote(show_id, &remote).await?;
                show_by_remote.insert(remote_id.clone(), show_id);
            }

            show_id
        };

        show_by_uuid.insert(s.id, show_id);

        if (i + 1) % 50 == 0 || i + 1 == total_show {
            tracing::info!("  Show {}/{total_show}", i + 1);
        }
    }

    tracing::info!("Importing seasons and episodes for {total_show} show");

    for (i, s) in all_shows.iter().enumerate() {
        let show_id = show_by_uuid[&s.id];

        let seasons_file = source.join("seasons").join(format!("{}.yaml", s.id));
        if seasons_file.exists() {
            let seasons: Vec<YamlSeason> = parse_yaml_docs(&seasons_file)
                .with_context(|| anyhow!("Parsing seasons for {}", s.id))?;

            for season in seasons {
                let air_date = season
                    .air_date
                    .as_ref()
                    .map(|d| naive_to_date(*d).to_timestamp_at_midnight_utc())
                    .transpose()?;

                db.upsert_season(
                    show_id,
                    season.number.into(),
                    air_date,
                    season.name.as_deref().filter(|s| !s.trim().is_empty()),
                    season.overview.as_deref().filter(|s| !s.trim().is_empty()),
                )
                .await
                .with_context(|| anyhow!("Inserting season for show {}", s.id))?;
            }
        }

        let episodes_file = source.join("episodes").join(format!("{}.yaml", s.id));
        if episodes_file.exists() {
            let episodes: Vec<YamlEpisode> = parse_yaml_docs(&episodes_file)
                .with_context(|| anyhow!("Parsing episodes for {}", s.id))?;

            for ep in episodes {
                let aired = ep
                    .aired
                    .as_ref()
                    .map(|d| naive_to_date(*d).to_timestamp_at_midnight_utc())
                    .transpose()?;

                db.upsert_episode(
                    api::EpisodeId::new(uuid_to_u64(ep.id)),
                    show_id,
                    ep.season.into(),
                    ep.number,
                    ep.absolute_number,
                    ep.name.as_deref().filter(|s| !s.trim().is_empty()),
                    ep.overview.as_deref().filter(|s| !s.trim().is_empty()),
                    aired,
                    remote_id(ep.remote_id.as_ref()).as_ref(),
                )
                .await
                .with_context(|| anyhow!("Inserting episode {} for show {}", ep.number, s.id))?;
            }
        }

        if (i + 1) % 50 == 0 || i + 1 == total_show {
            tracing::info!("  Seasons/episodes {}/{total_show} show", i + 1);
        }
    }

    let movies_path = source.join("movies.yaml");
    let all_movies: Vec<YamlMovie> =
        parse_yaml_docs(&movies_path).context("Parsing movies.yaml")?;

    let total_movies = all_movies.len();
    tracing::info!("Importing {total_movies} movies");

    for (i, m) in all_movies.iter().enumerate() {
        let already_exists = m
            .remote_id
            .as_ref()
            .is_some_and(|rid| movies_by_remote.contains_key(rid.as_str()));

        if !already_exists {
            let release_date = m
                .release_date
                .as_ref()
                .map(|d| naive_to_date(*d).to_timestamp_at_midnight_utc())
                .transpose()?;

            let movie_id = api::MovieId::new(uuid_to_u64(m.id));

            db.create_movie(movie_id, &m.title, release_date, &m.overview, true)
                .await
                .with_context(|| anyhow!("Inserting movie '{}'", m.title))?;

            import_movie_images(&db, movie_id, &m.graphics).await?;

            if let Some(rid) = &m.remote_id {
                let remote = api::RemoteId::from_raw(rid.as_str());
                db.add_movie_remote(movie_id, &remote).await?;
                movies_by_remote.insert(rid.clone(), movie_id);
            }
        }

        if (i + 1) % 20 == 0 || i + 1 == total_movies {
            tracing::info!("  Movies {}/{total_movies}", i + 1);
        }
    }

    let watched_path = source.join("watched.yaml");
    let all_watched: Vec<YamlWatched> =
        parse_yaml_docs(&watched_path).context("Parsing watched.yaml")?;

    let total_watched = all_watched.len();
    tracing::info!("Importing {total_watched} watched entries");

    for (i, w) in all_watched.into_iter().enumerate() {
        match w {
            YamlWatched::Episode {
                id,
                timestamp,
                series,
                place,
            } => {
                let Some((season, ep_number)) = parse_place(&place) else {
                    tracing::warn!("Skipping watched entry with unparseable place {place:?}");
                    continue;
                };

                let timestamp =
                    chrono_to_timestamp(timestamp).context("Parsing watched timestamp")?;

                db.insert_watched_episode(
                    api::WatchedId::new(uuid_to_u64(id)),
                    timestamp,
                    api::ShowId::new(uuid_to_u64(series)),
                    season,
                    ep_number,
                )
                .await
                .with_context(|| anyhow!("Inserting watched episode {place}"))?;
            }
            YamlWatched::Movie {
                id,
                timestamp,
                movie,
            } => {
                let timestamp =
                    chrono_to_timestamp(timestamp).context("Parsing watched timestamp")?;

                db.insert_watched_movie(
                    api::WatchedId::new(uuid_to_u64(id)),
                    timestamp,
                    api::MovieId::new(uuid_to_u64(movie)),
                )
                .await
                .context("Inserting watched movie")?;
            }
        }

        if (i + 1) % 1000 == 0 || i + 1 == total_watched {
            tracing::info!("  Watched {}/{total_watched}", i + 1);
        }
    }

    let remotes_path = source.join("remotes.yaml");

    if remotes_path.exists() {
        let all_remotes: Vec<YamlRemote> =
            parse_yaml_docs(&remotes_path).context("Parsing remotes.yaml")?;

        tracing::info!("Importing remotes from {} entries", all_remotes.len());
        let mut added = 0usize;

        for entry in &all_remotes {
            match entry.kind.as_str() {
                "show" => {
                    let show_id = api::ShowId::new(uuid_to_u64(entry.uuid));
                    for rid in &entry.remotes {
                        let remote = api::RemoteId::from_raw(rid.as_str());
                        db.add_show_remote(show_id, &remote)
                            .await
                            .with_context(|| {
                                anyhow!("Adding remote {rid} to show {:?}", entry.uuid)
                            })?;
                        added += 1;
                    }
                }
                "movie" => {
                    let movie_id = api::MovieId::new(uuid_to_u64(entry.uuid));
                    for rid in &entry.remotes {
                        let remote = api::RemoteId::from_raw(rid.as_str());
                        db.add_movie_remote(movie_id, &remote)
                            .await
                            .with_context(|| {
                                anyhow!("Adding remote {rid} to movie {:?}", entry.uuid)
                            })?;
                        added += 1;
                    }
                }
                _ => {}
            }
        }

        tracing::info!("Added {added} remote IDs");
    }

    let now = api::Timestamp::now();

    tracing::info!("Filling pending episodes for {} show", show_by_uuid.len());
    let mut pending_filled = 0usize;

    for &show_id in show_by_uuid.values() {
        db.fill_pending_for_show_import(show_id).await?;
        pending_filled += 1;
    }

    tracing::info!("Filled pending for {pending_filled} show");

    tracing::info!("Discovering pending movies");

    for (id, ts) in db.theatrical_movie_candidates(now).await? {
        let ts = ts.unwrap_or(now);
        db.add_pending_movie(id, ts).await?;
    }

    for (id, ts) in db.digital_movie_candidates(now).await? {
        let ts = ts.unwrap_or(now);
        db.add_pending_movie(id, ts).await?;
    }

    tracing::info!("Import complete");
    Ok(())
}
