use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use api::SeasonNumber;
use db::Database;
use musli_web::api::ChannelId;
use tokio::sync::{Mutex, broadcast};

use crate::remote::RemoteClients;

pub(crate) type SyncHandle = Arc<Mutex<HashSet<api::SeriesId>>>;

pub(crate) fn new_sync_handle() -> SyncHandle {
    Arc::new(Mutex::new(HashSet::new()))
}

fn broadcast_event(tx: &broadcast::Sender<api::AppEvent>, kind: api::AppEventKind) {
    let _ = tx.send(api::AppEvent {
        channel: ChannelId::NONE,
        kind,
    });
}

pub(crate) async fn sync_series(
    series_id: api::SeriesId,
    db: Database,
    remote: RemoteClients,
    broadcast: broadcast::Sender<api::AppEvent>,
    handle: SyncHandle,
) {
    {
        let mut set = handle.lock().await;
        if !set.insert(series_id) {
            return;
        }
    }

    broadcast_event(
        &broadcast,
        api::AppEventKind::SyncStarted {
            series_id: Some(series_id),
        },
    );

    if let Err(e) = do_sync_series(series_id, &db, &remote, &broadcast).await {
        tracing::error!(?series_id, error = %e, "sync_series failed");
    }

    broadcast_event(
        &broadcast,
        api::AppEventKind::SyncFinished {
            series_id: Some(series_id),
        },
    );
    broadcast_event(&broadcast, api::AppEventKind::PendingChanged);

    handle.lock().await.remove(&series_id);
}

async fn do_sync_series(
    series_id: api::SeriesId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &broadcast::Sender<api::AppEvent>,
) -> Result<()> {
    let series = db
        .series_by_id(series_id)
        .await?
        .context("series not found")?;
    let remote_id = series.remote_id.as_ref().context("series has no remote_id")?;

    match remote_id.source() {
        "tmdb" => {
            let tmdb_id: u32 = remote_id.value().parse().context("invalid tmdb id")?;
            sync_series_tmdb(series_id, tmdb_id, remote, db, broadcast).await?;
        }
        "tvdb" => {
            let tvdb_id: u32 = remote_id.value().parse().context("invalid tvdb id")?;
            sync_series_tvdb(series_id, tvdb_id, remote, db, broadcast).await?;
        }
        other => anyhow::bail!("unknown remote source: {other}"),
    }

    Ok(())
}

async fn sync_series_tmdb(
    series_id: api::SeriesId,
    tmdb_id: u32,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &broadcast::Sender<api::AppEvent>,
) -> Result<()> {
    let info = remote.fetch_tmdb_series(tmdb_id).await?;

    let series = db
        .series_by_id(series_id)
        .await?
        .context("series not found")?;
    db.update_series(
        series_id,
        &info.title,
        info.first_air_date.as_ref(),
        &info.overview,
        info.poster.as_ref(),
        None,
        info.fanart.as_ref(),
        series.tracked,
        series.remote_id.as_ref(),
    )
    .await?;

    let updated = db
        .series_by_id(series_id)
        .await?
        .context("series not found after update")?;
    broadcast_event(broadcast, api::AppEventKind::SeriesChanged { series: updated });

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

        for ep in remote.fetch_tmdb_season_episodes(tmdb_id, season_num).await? {
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
    broadcast_event(broadcast, api::AppEventKind::SeasonsChanged { series_id, seasons });

    Ok(())
}

async fn sync_series_tvdb(
    series_id: api::SeriesId,
    tvdb_id: u32,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &broadcast::Sender<api::AppEvent>,
) -> Result<()> {
    let info = remote.fetch_tvdb_series(tvdb_id).await?;

    let series = db
        .series_by_id(series_id)
        .await?
        .context("series not found")?;
    db.update_series(
        series_id,
        &info.title,
        None,
        &info.overview,
        info.poster.as_ref(),
        info.banner.as_ref(),
        info.fanart.as_ref(),
        series.tracked,
        series.remote_id.as_ref(),
    )
    .await?;

    let updated = db
        .series_by_id(series_id)
        .await?
        .context("series not found after update")?;
    broadcast_event(broadcast, api::AppEventKind::SeriesChanged { series: updated });

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
        db.upsert_season(series_id, season, None, None, "", None).await?;
        broadcast_event(broadcast, api::AppEventKind::EpisodesChanged { series_id, season });
    }

    let seasons = db.seasons(series_id).await?;
    broadcast_event(broadcast, api::AppEventKind::SeasonsChanged { series_id, seasons });

    Ok(())
}

pub(crate) async fn sync_movie(
    movie_id: api::MovieId,
    db: Database,
    remote: RemoteClients,
    broadcast: broadcast::Sender<api::AppEvent>,
) {
    if let Err(e) = do_sync_movie(movie_id, &db, &remote, &broadcast).await {
        tracing::error!(?movie_id, error = %e, "sync_movie failed");
    }
}

async fn do_sync_movie(
    movie_id: api::MovieId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &broadcast::Sender<api::AppEvent>,
) -> Result<()> {
    let movie = db.movie_by_id(movie_id).await?.context("movie not found")?;
    let remote_id = movie.remote_id.as_ref().context("movie has no remote_id")?;

    match remote_id.source() {
        "tmdb" => {
            let tmdb_id: u32 = remote_id.value().parse().context("invalid tmdb id")?;
            let info = remote.fetch_tmdb_movie(tmdb_id).await?;

            db.update_movie(
                movie_id,
                &info.title,
                info.release_date.as_ref(),
                &info.overview,
                info.poster.as_ref(),
                None,
                info.fanart.as_ref(),
                movie.remote_id.as_ref(),
            )
            .await?;

            let updated = db
                .movie_by_id(movie_id)
                .await?
                .context("movie not found after update")?;
            broadcast_event(broadcast, api::AppEventKind::MovieChanged { movie: updated });
        }
        other => anyhow::bail!("unsupported movie remote source: {other}"),
    }

    broadcast_event(broadcast, api::AppEventKind::PendingChanged);
    Ok(())
}
