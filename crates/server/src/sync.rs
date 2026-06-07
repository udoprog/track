use std::collections::HashSet;

use anyhow::{Context as _, Result};
use api::{ImageKind, ImageSource, SeasonNumber};
use db::Database;
use musli_web::api::ChannelId;
use tokio::sync::broadcast;

use crate::remote::RemoteClients;

fn broadcast_event(tx: &broadcast::Sender<api::AppEvent>, kind: api::AppEventKind) {
    let _ = tx.send(api::AppEvent {
        channel: ChannelId::NONE,
        kind,
    });
}

pub(crate) async fn sync_series(
    series_id: api::SeriesId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &broadcast::Sender<api::AppEvent>,
) -> Result<()> {
    let series = db
        .series_by_id(series_id)
        .await?
        .context("series not found")?;

    let source = series.effective_sync_source();

    match source {
        Some(api::SyncSource::Tmdb) => {
            let remote_id = series
                .remote_by_source("tmdb")
                .context("series has no tmdb remote")?;
            let tmdb_id: u32 = remote_id.value().parse().context("invalid tmdb id")?;
            sync_series_tmdb(series_id, tmdb_id, remote, db, broadcast).await?;
        }
        Some(api::SyncSource::Tvdb) => {
            let remote_id = series
                .remote_by_source("tvdb")
                .context("series has no tvdb remote")?;
            let tvdb_id: u32 = remote_id.value().parse().context("invalid tvdb id")?;
            sync_series_tvdb(series_id, tvdb_id, remote, db, broadcast).await?;
        }
        None => anyhow::bail!("series has no syncable remote (tmdb or tvdb)"),
    }

    broadcast_event(broadcast, api::AppEventKind::PendingChanged);
    Ok(())
}

async fn sync_series_tmdb(
    series_id: api::SeriesId,
    tmdb_id: u32,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &broadcast::Sender<api::AppEvent>,
) -> Result<()> {
    let series = db
        .series_by_id(series_id)
        .await?
        .context("series not found")?;
    let info = remote.fetch_tmdb_series(tmdb_id).await?;

    db.update_series(
        series_id,
        &info.title,
        info.first_air_date
            .as_ref()
            .or(series.first_air_date.as_ref()),
        &info.overview,
        series.tracked,
    )
    .await?;

    if let Some(ref img) = info.poster {
        db.upsert_series_image(series_id, ImageKind::Poster, ImageSource::Tmdb, img.path())
            .await?;
    }
    if let Some(ref img) = info.fanart {
        db.upsert_series_image(
            series_id,
            ImageKind::Backdrop,
            ImageSource::Tmdb,
            img.path(),
        )
        .await?;
    }

    let updated = db
        .series_by_id(series_id)
        .await?
        .context("series not found after update")?;
    broadcast_event(
        broadcast,
        api::AppEventKind::SeriesChanged { series: updated },
    );

    for season_info in &info.seasons {
        db.upsert_season(
            series_id,
            season_info.number,
            season_info.air_date.as_ref(),
            season_info.name.as_deref(),
            &season_info.overview,
            season_info.poster.as_ref(),
        )
        .await?;

        let season_num = match season_info.number {
            SeasonNumber::Specials => 0,
            SeasonNumber::Number(n) => n,
        };

        for ep in remote
            .fetch_tmdb_season_episodes(tmdb_id, season_num)
            .await?
        {
            db.upsert_episode(
                series_id,
                ep.season,
                ep.number,
                None,
                ep.name.as_deref(),
                &ep.overview,
                ep.aired.as_ref(),
                ep.filename.as_ref(),
                Some(&ep.remote_id),
            )
            .await?;
        }

        broadcast_event(
            broadcast,
            api::AppEventKind::EpisodesChanged {
                series_id,
                season: season_info.number,
            },
        );
    }

    let seasons = db.seasons(series_id).await?;
    broadcast_event(
        broadcast,
        api::AppEventKind::SeasonsChanged { series_id, seasons },
    );

    Ok(())
}

async fn sync_series_tvdb(
    series_id: api::SeriesId,
    tvdb_id: u32,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &broadcast::Sender<api::AppEvent>,
) -> Result<()> {
    let series = db
        .series_by_id(series_id)
        .await?
        .context("series not found")?;
    let info = remote.fetch_tvdb_series(tvdb_id).await?;

    db.update_series(
        series_id,
        &info.title,
        series.first_air_date.as_ref(),
        &info.overview,
        series.tracked,
    )
    .await?;

    if let Some(ref img) = info.poster {
        db.upsert_series_image(series_id, ImageKind::Poster, ImageSource::Tvdb, img.path())
            .await?;
    }
    if let Some(ref img) = info.banner {
        db.upsert_series_image(series_id, ImageKind::Banner, ImageSource::Tvdb, img.path())
            .await?;
    }
    if let Some(ref img) = info.fanart {
        db.upsert_series_image(series_id, ImageKind::Fanart, ImageSource::Tvdb, img.path())
            .await?;
    }

    let updated = db
        .series_by_id(series_id)
        .await?
        .context("series not found after update")?;
    broadcast_event(
        broadcast,
        api::AppEventKind::SeriesChanged { series: updated },
    );

    let episodes = remote.fetch_tvdb_episodes(tvdb_id).await?;

    let mut seasons_seen: HashSet<SeasonNumber> = HashSet::new();
    for ep in &episodes {
        seasons_seen.insert(ep.season);
        db.upsert_episode(
            series_id,
            ep.season,
            ep.number,
            ep.absolute_number,
            ep.name.as_deref(),
            &ep.overview,
            ep.aired.as_ref(),
            ep.filename.as_ref(),
            Some(&ep.remote_id),
        )
        .await?;
    }

    for &season in &seasons_seen {
        db.upsert_season(series_id, season, None, None, "", None)
            .await?;
        broadcast_event(
            broadcast,
            api::AppEventKind::EpisodesChanged { series_id, season },
        );
    }

    let seasons = db.seasons(series_id).await?;
    broadcast_event(
        broadcast,
        api::AppEventKind::SeasonsChanged { series_id, seasons },
    );

    Ok(())
}

pub(crate) async fn sync_movie(
    movie_id: api::MovieId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &broadcast::Sender<api::AppEvent>,
) -> Result<()> {
    let movie = db.movie_by_id(movie_id).await?.context("movie not found")?;

    let source = movie.effective_sync_source();

    match source {
        Some(api::SyncSource::Tmdb) => {
            let remote_id = movie
                .remote_by_source("tmdb")
                .context("movie has no tmdb remote")?;
            let tmdb_id: u32 = remote_id.value().parse().context("invalid tmdb id")?;
            let info = remote.fetch_tmdb_movie(tmdb_id).await?;

            db.update_movie(
                movie_id,
                &info.title,
                info.release_date.as_ref().or(movie.release_date.as_ref()),
                &info.overview,
            )
            .await?;

            if let Some(ref img) = info.poster {
                db.upsert_movie_image(movie_id, ImageKind::Poster, ImageSource::Tmdb, img.path())
                    .await?;
            }
            if let Some(ref img) = info.fanart {
                db.upsert_movie_image(movie_id, ImageKind::Backdrop, ImageSource::Tmdb, img.path())
                    .await?;
            }

            let updated = db
                .movie_by_id(movie_id)
                .await?
                .context("movie not found after update")?;
            broadcast_event(
                broadcast,
                api::AppEventKind::MovieChanged { movie: updated },
            );
        }
        Some(api::SyncSource::Tvdb) => anyhow::bail!("unsupported movie sync source: tvdb"),
        None => anyhow::bail!("movie has no syncable remote (tmdb)"),
    }

    broadcast_event(broadcast, api::AppEventKind::PendingChanged);
    Ok(())
}
