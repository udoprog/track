use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result};
use api::{EpisodeId, Image, ImageId, ImageKind, SeasonNumber};
use tracing::{info, warn};

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::remote::RemoteClients;

pub(crate) async fn sync_show(
    show_id: api::ShowId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
    pending: &crate::pending::PendingSystem,
) -> Result<()> {
    let show = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist")?;

    let config = db.load_config().await?;
    let language = show.language.as_deref().or(config.language.as_deref());

    let source = show.effective_sync_source();
    info!(show_id = %show_id, title = show.title, ?source, ?language, "Syncing show");

    match source {
        Some(api::SyncSource::Tmdb) => {
            let remote_id = show
                .remote_by_source(api::SyncSource::Tmdb)
                .context("Expected show to have a TMDB remote")?;

            let tmdb_id: u32 = remote_id
                .value()
                .as_u32()
                .context("Expected a valid TMDB id")?;

            sync_show_tmdb(show_id, tmdb_id, language, remote, db, broadcast).await?;
        }
        Some(api::SyncSource::Tvdb) => {
            let remote_id = show
                .remote_by_source(api::SyncSource::Tvdb)
                .context("Expected show to have a TVDB remote")?;

            let tvdb_id: u32 = remote_id
                .value()
                .as_u32()
                .context("Expected a valid TVDB id")?;

            sync_show_tvdb(show_id, tvdb_id, language, remote, db, broadcast).await?;
        }
        _ => anyhow::bail!("Show has no syncable remote (TMDB or TVDB)"),
    }

    // Best-effort tvmaze enrichment for exact airtimes. Re-fetch so remotes are
    // current.
    if let Some(show) = db.show_by_id(show_id).await?
        && let Err(e) = enrich_with_tvmaze(show_id, &show, remote, db, broadcast).await
    {
        warn!("TVmaze enrichment skipped for show {show_id}: {e:#}");
    }

    let now = api::Timestamp::now();
    let include_specials = show.effective_include_specials(config.include_specials);
    pending
        .fill_for_show(show_id, include_specials, now)
        .await?;
    db.set_show_synced_at(show_id, now).await?;
    broadcast.broadcast_event(api::AppEventKind::PendingChanged);
    info!(show_id = %show_id, "Sync complete");
    Ok(())
}

async fn sync_show_tmdb(
    show_id: api::ShowId,
    tmdb_id: u32,
    language: Option<&str>,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    let show = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist")?;

    info!(tmdb_id, "Fetching TMDB show");

    let info = remote.fetch_tmdb_show(tmdb_id, language).await?;

    db.update_show(
        show_id,
        info.title.as_deref(),
        info.first_air_date.or(show.first_air_date),
        info.overview.as_deref(),
        show.tracked,
    )
    .await?;

    for remote in &info.remotes {
        db.add_show_remote(show_id, remote).await?;
    }

    db.clear_show_images(show_id).await?;

    let mut selected_poster_id = None;
    let mut selected_backdrop_id = None;

    for (rank, poster) in info.posters.iter().enumerate() {
        let id = ImageId::random();

        db.upsert_show_image(id, show_id, ImageKind::Poster, rank as u32, poster)
            .await?;

        if info.selected_poster.as_ref() == Some(poster.key()) {
            selected_poster_id = Some(id);
        }
    }

    for (rank, backdrop) in info.backdrops.iter().enumerate() {
        let id = ImageId::random();

        db.upsert_show_image(id, show_id, ImageKind::Backdrop, rank as u32, backdrop)
            .await?;

        if info.selected_backdrop.as_ref() == Some(backdrop.key()) {
            selected_backdrop_id = Some(id);
        }
    }

    if let Some(id) = selected_poster_id {
        db.set_show_image_selection(show_id, ImageKind::Poster, id)
            .await?;
    }

    if let Some(id) = selected_backdrop_id {
        db.set_show_image_selection(show_id, ImageKind::Backdrop, id)
            .await?;

        db.set_show_image_selection(show_id, ImageKind::Banner, id)
            .await?;
    }

    let updated = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist after update")?;
    broadcast.broadcast_event(api::AppEventKind::ShowChanged { show: updated });

    let mut synced_seasons = HashSet::new();

    db.clear_episode_images(show_id).await?;

    let existing_episode_ids = db.episode_ids(show_id).await?;

    for info in &info.seasons {
        db.upsert_season(
            show_id,
            info.number,
            info.air_date,
            info.name.as_deref(),
            info.overview.as_deref(),
        )
        .await?;

        info!(tmdb_id, season = ?info.number, "Fetching TMDB season episodes");

        let mut fetched_numbers = HashSet::new();

        for ep in remote
            .fetch_tmdb_season_episodes(tmdb_id, info.number, language)
            .await?
        {
            fetched_numbers.insert(ep.number);

            let episode_id = existing_episode_ids
                .get(&(ep.season, ep.number))
                .copied()
                .unwrap_or_else(EpisodeId::random);

            db.upsert_episode(
                episode_id,
                show_id,
                ep.season,
                ep.number,
                None,
                ep.name.as_deref(),
                ep.overview.as_deref(),
                ep.aired,
                Some(&ep.remote_id),
            )
            .await?;

            if let Some(path) = &ep.filename {
                let image_id = ImageId::random();
                let image = Image::from(path.clone());

                db.upsert_episode_image(image_id, episode_id, ImageKind::Screenshot, &image)
                    .await?;

                db.set_episode_image_selection(episode_id, ImageKind::Screenshot, image_id)
                    .await?;
            }
        }

        db.prune_season_episodes(show_id, info.number, &fetched_numbers)
            .await?;

        broadcast.broadcast_event(api::AppEventKind::EpisodesChanged {
            show_id,
            season: info.number,
        });

        synced_seasons.insert(info.number);
    }

    db.prune_seasons(show_id, &synced_seasons).await?;

    let seasons = db.seasons(show_id).await?;
    broadcast.broadcast_event(api::AppEventKind::SeasonsChanged { show_id, seasons });

    Ok(())
}

async fn sync_show_tvdb(
    show_id: api::ShowId,
    tvdb_id: u32,
    language: Option<&str>,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    let show = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist")?;

    info!(tvdb_id, "Fetching TVDB show");

    let info = remote.fetch_tvdb_show(tvdb_id, language).await?;

    db.update_show(
        show_id,
        info.title.as_deref(),
        show.first_air_date,
        info.overview.as_deref(),
        show.tracked,
    )
    .await?;

    for remote in &info.remotes {
        db.add_show_remote(show_id, remote).await?;
    }

    db.clear_show_images(show_id).await?;

    let mut selected_poster_id = None;
    let mut selected_banner_id = None;
    let mut selected_fanart_id = None;

    for (rank, poster) in info.poster.iter().enumerate() {
        let id = ImageId::random();

        db.upsert_show_image(id, show_id, ImageKind::Poster, rank as u32, poster)
            .await?;

        if info.selected_poster.as_ref() == Some(poster.key()) {
            selected_poster_id = Some(id);
        }
    }

    for (rank, banner) in info.banner.iter().enumerate() {
        let id = ImageId::random();

        db.upsert_show_image(id, show_id, ImageKind::Banner, rank as u32, banner)
            .await?;

        if info.selected_banner.as_ref() == Some(banner.key()) {
            selected_banner_id = Some(id);
        }
    }

    for (rank, fanart) in info.fanart.iter().enumerate() {
        let id = ImageId::random();

        db.upsert_show_image(id, show_id, ImageKind::Backdrop, rank as u32, fanart)
            .await?;

        if info.selected_fanart.as_ref() == Some(fanart.key()) {
            selected_fanart_id = Some(id);
        }
    }

    if let Some(id) = selected_poster_id {
        db.set_show_image_selection(show_id, ImageKind::Poster, id)
            .await?;
    }

    if let Some(id) = selected_banner_id {
        db.set_show_image_selection(show_id, ImageKind::Banner, id)
            .await?;
    }

    if let Some(id) = selected_fanart_id {
        db.set_show_image_selection(show_id, ImageKind::Backdrop, id)
            .await?;
    }

    let updated = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist after update")?;

    broadcast.broadcast_event(api::AppEventKind::ShowChanged { show: updated });

    info!(tvdb_id, "Fetching TVDB episodes");
    let episodes = remote.fetch_tvdb_episodes(tvdb_id, language).await?;
    info!(count = episodes.len(), "Got episodes from TVDB");

    let mut seasons_seen: HashSet<SeasonNumber> = HashSet::new();
    let mut season_air_dates: HashMap<SeasonNumber, api::Timestamp> = HashMap::new();
    let mut season_episode_numbers: HashMap<SeasonNumber, HashSet<u32>> = HashMap::new();

    db.clear_episode_images(show_id).await?;

    let existing_episode_ids = db.episode_ids(show_id).await?;

    for ep in &episodes {
        seasons_seen.insert(ep.season);
        season_episode_numbers
            .entry(ep.season)
            .or_default()
            .insert(ep.number);

        if let Some(aired) = ep.aired {
            let entry = season_air_dates.entry(ep.season).or_insert(aired);

            if aired < *entry {
                *entry = aired;
            }
        }

        let episode_id = existing_episode_ids
            .get(&(ep.season, ep.number))
            .copied()
            .unwrap_or_else(EpisodeId::random);

        db.upsert_episode(
            episode_id,
            show_id,
            ep.season,
            ep.number,
            ep.absolute_number,
            ep.name.as_deref(),
            ep.overview.as_deref(),
            ep.aired,
            Some(&ep.remote_id),
        )
        .await?;

        if let Some((source, path)) = &ep.image {
            let image_id = ImageId::random();

            db.upsert_episode_image(
                image_id,
                episode_id,
                ImageKind::Screenshot,
                &api::Image::new(*source, path),
            )
            .await?;

            db.set_episode_image_selection(episode_id, ImageKind::Screenshot, image_id)
                .await?;
        }
    }

    for &season in &seasons_seen {
        let air_date = season_air_dates.get(&season).copied();

        db.upsert_season(show_id, season, air_date, None, None)
            .await?;

        if let Some(kept) = season_episode_numbers.get(&season) {
            db.prune_season_episodes(show_id, season, kept).await?;
        }

        broadcast.broadcast_event(api::AppEventKind::EpisodesChanged { show_id, season });
    }

    db.prune_seasons(show_id, &seasons_seen).await?;

    let seasons = db.seasons(show_id).await?;
    broadcast.broadcast_event(api::AppEventKind::SeasonsChanged { show_id, seasons });

    Ok(())
}

pub(crate) async fn sync_movie(
    movie_id: api::MovieId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
) -> Result<()> {
    let movie = db
        .movie_by_id(movie_id)
        .await?
        .context("Expected movie to exist")?;

    let config = db.load_config().await?;
    let language = movie.language.as_deref().or(config.language.as_deref());

    let source = movie.effective_sync_source();
    info!(movie_id = %movie_id, title = movie.title, ?source, ?language, "Syncing movie");

    match source {
        Some(api::SyncSource::Tmdb) => {
            let remote_id = movie
                .remote_by_source(api::SyncSource::Tmdb)
                .context("Expected movie to have a TMDB remote")?;

            let tmdb_id: u32 = remote_id
                .value()
                .as_u32()
                .context("Expected a valid TMDB id")?;
            info!(tmdb_id, "Fetching TMDB movie");

            let info = remote.fetch_tmdb_movie(tmdb_id, language).await?;

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

            db.clear_movie_images(movie_id).await?;

            let mut selected_poster_id = None;
            let mut selected_backdrop_id = None;

            for (rank, img) in info.posters.iter().enumerate() {
                let id = ImageId::random();

                db.upsert_movie_image(id, movie_id, ImageKind::Poster, rank as u32, img)
                    .await?;

                if info.selected_poster.as_ref() == Some(img.key()) {
                    selected_poster_id = Some(id);
                }
            }

            for (rank, img) in info.backdrops.iter().enumerate() {
                let id = ImageId::random();

                db.upsert_movie_image(id, movie_id, ImageKind::Backdrop, rank as u32, img)
                    .await?;

                if info.selected_backdrop.as_ref() == Some(img.key()) {
                    selected_backdrop_id = Some(id);
                }
            }

            if let Some(id) = selected_poster_id {
                db.set_movie_image_selection(movie_id, ImageKind::Poster, id)
                    .await?;
            }

            if let Some(id) = selected_backdrop_id {
                db.set_movie_image_selection(movie_id, ImageKind::Backdrop, id)
                    .await?;

                db.set_movie_image_selection(movie_id, ImageKind::Banner, id)
                    .await?;
            }

            match remote.fetch_tmdb_movie_releases(tmdb_id).await {
                Ok(releases) => {
                    info!(count = releases.len(), "Fetched TMDB movie releases");

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
                Err(e) => warn!(movie_id = %movie_id, "Movie release dates skipped: {e:#}"),
            }

            let updated = db
                .movie_by_id(movie_id)
                .await?
                .context("Expected movie to exist after update")?;

            broadcast.broadcast_event(api::AppEventKind::MovieChanged { movie: updated });
        }
        Some(api::SyncSource::Tvdb) => anyhow::bail!("Unsupported movie sync source: TVDB"),
        _ => anyhow::bail!("Movie has no syncable remote"),
    }

    crate::background::discover_pending_movies(db).await?;
    db.set_movie_synced_at(movie_id, api::Timestamp::now())
        .await?;
    broadcast.broadcast_event(api::AppEventKind::PendingChanged);
    info!(movie_id = %movie_id, "Sync complete");
    Ok(())
}

#[tracing::instrument(skip_all, fields(show_id = %show_id))]
async fn enrich_with_tvmaze(
    show_id: api::ShowId,
    show: &api::Show,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    let tvmaze_id = 'id: {
        if let Some(r) = show
            .remotes
            .iter()
            .find(|r| *r.source() == api::RemoteSource::Tvdb)
        {
            let id: u32 = r.value().as_u32().context("Expected a valid TVDB id")?;
            info!(tvdb_id = id, "Looking up TVmaze id via TVDB");
            break 'id remote.lookup_tvmaze_by_tvdb(id).await?;
        }

        if let Some(r) = show
            .remotes
            .iter()
            .find(|r| *r.source() == api::RemoteSource::Imdb)
        {
            let imdb_id = r.value().as_str().context("Expected a valid IMDB id")?;
            info!(imdb_id, "Looking up TVmaze id via IMDB");
            break 'id remote.lookup_tvmaze_by_imdb(imdb_id).await?;
        }

        info!(show_id = %show_id, "Skipping TVmaze enrichment: no TVDB or IMDB remote");
        return Ok(());
    };

    let Some(tvmaze_id) = tvmaze_id else {
        info!(show_id = %show_id, "Skipping TVmaze enrichment: not found on TVmaze");
        return Ok(());
    };

    info!(tvmaze_id, "Fetching TVmaze episodes");

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
        "Updating episodes with exact airtimes"
    );

    db.update_episodes_aired(show_id, updates).await?;

    for season in seasons_updated {
        broadcast.broadcast_event(api::AppEventKind::EpisodesChanged { show_id, season });
    }

    Ok(())
}
