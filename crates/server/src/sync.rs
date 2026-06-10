use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result};
use api::{EpisodeId, ImageKind, ImageSource, SeasonNumber};
use db::Database;
use tracing::{info, warn};

use crate::app_broadcast::Broadcaster;
use crate::remote::RemoteClients;

pub(crate) async fn sync_series(
    series_id: api::SeriesId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
    pending: &crate::pending::PendingSystem,
) -> Result<()> {
    let series = db
        .series_by_id(series_id)
        .await?
        .context("series not found")?;

    let config = db.load_config().await?;
    let language = series.language.as_deref().or(config.language.as_deref());

    let source = series.effective_sync_source();
    info!(series_id = %series_id, title = series.title, ?source, ?language, "syncing series");

    match source {
        Some(api::SyncSource::Tmdb) => {
            let remote_id = series
                .remote_by_source("tmdb")
                .context("series has no tmdb remote")?;
            let tmdb_id: u32 = remote_id.value().as_u32().context("invalid tmdb id")?;

            sync_series_tmdb(series_id, tmdb_id, language, remote, db, broadcast).await?;
        }
        Some(api::SyncSource::Tvdb) => {
            let remote_id = series
                .remote_by_source("tvdb")
                .context("series has no tvdb remote")?;
            let tvdb_id: u32 = remote_id.value().as_u32().context("invalid tvdb id")?;

            sync_series_tvdb(series_id, tvdb_id, language, remote, db, broadcast).await?;
        }
        None => anyhow::bail!("series has no syncable remote (tmdb or tvdb)"),
    }

    // Best-effort tvmaze enrichment for exact airtimes. Re-fetch so remotes are
    // current.
    if let Some(series) = db.series_by_id(series_id).await? {
        if let Err(e) = enrich_with_tvmaze(series_id, &series, remote, db, broadcast).await {
            warn!("tvmaze enrichment skipped for series {series_id}: {e:#}");
        }
    }

    let now = api::Timestamp::now();
    pending.fill_for_series(series_id, now).await?;
    db.set_series_synced_at(series_id, now).await?;
    broadcast.broadcast_event(api::AppEventKind::PendingChanged);
    info!(series_id = %series_id, "sync complete");
    Ok(())
}

async fn sync_series_tmdb(
    series_id: api::SeriesId,
    tmdb_id: u32,
    language: Option<&str>,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    let series = db
        .series_by_id(series_id)
        .await?
        .context("series not found")?;
    info!(tmdb_id, "fetching tmdb series");
    let info = remote.fetch_tmdb_series(tmdb_id, language).await?;

    db.update_series(
        series_id,
        info.title.as_deref(),
        info.first_air_date.or(series.first_air_date),
        info.overview.as_deref(),
        series.tracked,
    )
    .await?;

    for remote in &info.remotes {
        db.add_series_remote(series_id, remote).await?;
    }

    for image in info.posters {
        let selected = Some(&image) == info.selected_poster.as_ref();

        db.upsert_series_image(
            series_id,
            ImageKind::Poster,
            ImageSource::Tmdb,
            image.path(),
            selected,
        )
        .await?;
    }

    for image in info.backdrops {
        let selected = Some(&image) == info.selected_backdrop.as_ref();

        db.upsert_series_image(
            series_id,
            ImageKind::Backdrop,
            ImageSource::Tmdb,
            image.path(),
            selected,
        )
        .await?;
    }

    let updated = db
        .series_by_id(series_id)
        .await?
        .context("series not found after update")?;
    broadcast.broadcast_event(api::AppEventKind::SeriesChanged { series: updated });

    let mut synced_seasons: Vec<SeasonNumber> = Vec::new();

    for season_info in &info.seasons {
        db.upsert_season(
            series_id,
            season_info.number,
            season_info.air_date,
            season_info.name.as_deref(),
            season_info.overview.as_deref(),
            season_info.poster.as_ref(),
        )
        .await?;

        let season = match season_info.number {
            SeasonNumber::Specials => 0,
            SeasonNumber::Number(n) => n,
        };

        info!(tmdb_id, season, "fetching tmdb season episodes");

        for ep in remote
            .fetch_tmdb_season_episodes(tmdb_id, season, language)
            .await?
        {
            db.upsert_episode(
                EpisodeId::random(),
                series_id,
                ep.season,
                ep.number,
                None,
                ep.name.as_deref(),
                ep.overview.as_deref(),
                ep.aired,
                ep.filename.as_ref(),
                Some(&ep.remote_id),
            )
            .await?;
        }

        broadcast.broadcast_event(api::AppEventKind::EpisodesChanged {
            series_id,
            season: season_info.number,
        });

        synced_seasons.push(season_info.number);
    }

    db.prune_seasons(series_id, &synced_seasons).await?;

    let seasons = db.seasons(series_id).await?;
    broadcast.broadcast_event(api::AppEventKind::SeasonsChanged { series_id, seasons });

    Ok(())
}

async fn sync_series_tvdb(
    series_id: api::SeriesId,
    tvdb_id: u32,
    language: Option<&str>,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    let series = db
        .series_by_id(series_id)
        .await?
        .context("series not found")?;
    info!(tvdb_id, "fetching TVDB series");
    let info = remote.fetch_tvdb_series(tvdb_id, language).await?;

    db.update_series(
        series_id,
        info.title.as_deref(),
        series.first_air_date,
        info.overview.as_deref(),
        series.tracked,
    )
    .await?;

    for remote in &info.remotes {
        db.add_series_remote(series_id, remote).await?;
    }

    for poster in info.poster {
        let selected = Some(&poster) == info.selected_poster.as_ref();
        db.upsert_series_image(
            series_id,
            ImageKind::Poster,
            ImageSource::Tvdb,
            poster.path(),
            selected,
        )
        .await?;
    }

    for banner in info.banner {
        let selected = Some(&banner) == info.selected_banner.as_ref();
        db.upsert_series_image(
            series_id,
            ImageKind::Banner,
            ImageSource::Tvdb,
            banner.path(),
            selected,
        )
        .await?;
    }

    for fanart in info.fanart {
        let selected = Some(&fanart) == info.selected_fanart.as_ref();
        db.upsert_series_image(
            series_id,
            ImageKind::Fanart,
            ImageSource::Tvdb,
            fanart.path(),
            selected,
        )
        .await?;
    }

    let updated = db
        .series_by_id(series_id)
        .await?
        .context("series not found after update")?;

    broadcast.broadcast_event(api::AppEventKind::SeriesChanged { series: updated });

    info!(tvdb_id, "fetching TVDB episodes");
    let episodes = remote.fetch_tvdb_episodes(tvdb_id, language).await?;
    info!(count = episodes.len(), "got episodes from TVDB");

    let mut seasons_seen: HashSet<SeasonNumber> = HashSet::new();
    let mut season_air_dates: HashMap<SeasonNumber, api::Timestamp> = HashMap::new();

    for ep in &episodes {
        seasons_seen.insert(ep.season);

        if let Some(aired) = ep.aired {
            let entry = season_air_dates.entry(ep.season).or_insert(aired);

            if aired < *entry {
                *entry = aired;
            }
        }

        db.upsert_episode(
            EpisodeId::random(),
            series_id,
            ep.season,
            ep.number,
            ep.absolute_number,
            ep.name.as_deref(),
            ep.overview.as_deref(),
            ep.aired,
            ep.filename.as_ref(),
            Some(&ep.remote_id),
        )
        .await?;
    }

    for &season in &seasons_seen {
        let air_date = season_air_dates.get(&season).copied();

        db.upsert_season(series_id, season, air_date, None, None, None)
            .await?;

        broadcast.broadcast_event(api::AppEventKind::EpisodesChanged { series_id, season });
    }

    let synced_seasons: Vec<SeasonNumber> = seasons_seen.into_iter().collect();
    db.prune_seasons(series_id, &synced_seasons).await?;

    let seasons = db.seasons(series_id).await?;
    broadcast.broadcast_event(api::AppEventKind::SeasonsChanged { series_id, seasons });

    Ok(())
}

pub(crate) async fn sync_movie(
    movie_id: api::MovieId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
) -> Result<()> {
    let movie = db.movie_by_id(movie_id).await?.context("movie not found")?;

    let config = db.load_config().await?;
    let language = movie.language.as_deref().or(config.language.as_deref());

    let source = movie.effective_sync_source();
    info!(movie_id = %movie_id, title = movie.title, ?source, ?language, "syncing movie");

    match source {
        Some(api::SyncSource::Tmdb) => {
            let remote_id = movie
                .remote_by_source("tmdb")
                .context("movie has no tmdb remote")?;

            let tmdb_id: u32 = remote_id.value().as_u32().context("invalid tmdb id")?;
            info!(tmdb_id, "fetching tmdb movie");

            let info = remote
                .fetch_tmdb_movie(tmdb_id, language.as_deref())
                .await?;

            db.update_movie(
                movie_id,
                info.title.as_deref(),
                info.release_date.or(movie.release_date),
                info.overview.as_deref(),
            )
            .await?;

            for remote in &info.remotes {
                db.add_movie_remote(movie_id, remote).await?;
            }

            for img in info.posters {
                let selected = Some(&img) == info.selected_poster.as_ref();
                db.upsert_movie_image(
                    movie_id,
                    ImageKind::Poster,
                    ImageSource::Tmdb,
                    img.path(),
                    selected,
                )
                .await?;
            }

            for img in info.backdrops {
                let selected = Some(&img) == info.selected_backdrop.as_ref();
                db.upsert_movie_image(
                    movie_id,
                    ImageKind::Backdrop,
                    ImageSource::Tmdb,
                    img.path(),
                    selected,
                )
                .await?;
            }

            match remote.fetch_tmdb_movie_releases(tmdb_id).await {
                Ok(releases) => {
                    info!(count = releases.len(), "fetched tmdb movie releases");

                    for r in releases {
                        db.upsert_movie_release(
                            movie_id,
                            &r.country,
                            r.release_type,
                            &r.release_date,
                        )
                        .await?;
                    }
                }
                Err(e) => warn!(movie_id = %movie_id, "movie release dates skipped: {e:#}"),
            }

            let updated = db
                .movie_by_id(movie_id)
                .await?
                .context("movie not found after update")?;
            broadcast.broadcast_event(api::AppEventKind::MovieChanged { movie: updated });
        }
        Some(api::SyncSource::Tvdb) => anyhow::bail!("unsupported movie sync source: tvdb"),
        None => anyhow::bail!("movie has no syncable remote (tmdb)"),
    }

    crate::background::discover_pending_movies(db).await?;
    db.set_movie_synced_at(movie_id, api::Timestamp::now())
        .await?;
    broadcast.broadcast_event(api::AppEventKind::PendingChanged);
    info!(movie_id = %movie_id, "sync complete");
    Ok(())
}

#[tracing::instrument(skip_all, fields(series_id = %series_id))]
async fn enrich_with_tvmaze(
    series_id: api::SeriesId,
    series: &api::Series,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    let tvmaze_id = if let Some(r) = series.remote_by_source("tvdb") {
        let id: u32 = r.value().as_u32().context("invalid tvdb id")?;
        info!(tvdb_id = id, "looking up tvmaze id via TVDB");
        remote.lookup_tvmaze_by_tvdb(id).await?
    } else if let Some(r) = series.remote_by_source("imdb") {
        let imdb_id = r.value().as_str().context("invalid imdb id")?;
        info!(imdb_id, "looking up tvmaze id via IMDB");
        remote.lookup_tvmaze_by_imdb(imdb_id).await?
    } else {
        info!(series_id = %series_id, "skipping tvmaze enrichment: no TVDB or IMDB remote");
        return Ok(());
    };

    let Some(tvmaze_id) = tvmaze_id else {
        info!(series_id = %series_id, "skipping tvmaze enrichment: not found on tvmaze");
        return Ok(());
    };

    info!(tvmaze_id, "fetching tvmaze episodes");

    let tvmaze_eps = remote.fetch_tvmaze_episodes(tvmaze_id).await?;

    let mut seasons_updated: HashSet<SeasonNumber> = HashSet::new();
    let updates: Vec<(SeasonNumber, u32, api::Timestamp)> = tvmaze_eps
        .into_iter()
        .map(|ep| {
            seasons_updated.insert(ep.season);
            (ep.season, ep.number, ep.aired_at)
        })
        .collect();

    info!(
        episodes = updates.len(),
        seasons = seasons_updated.len(),
        "updating episodes with exact airtimes"
    );

    db.update_episodes_aired(series_id, updates).await?;

    for season in seasons_updated {
        broadcast.broadcast_event(api::AppEventKind::EpisodesChanged { series_id, season });
    }

    Ok(())
}
