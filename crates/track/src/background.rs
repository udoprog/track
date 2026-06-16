use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tracing::info;

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;

const POLL: Duration = Duration::from_secs(15 * 60);

pub(crate) async fn discover_pending_movies(db: &Database) -> anyhow::Result<()> {
    let now = api::Timestamp::now();
    let default = db.load_config().await?.release_filters;

    for (id, raw) in db.movie_pending_candidates().await? {
        let filters = raw.as_deref().and_then(api::decode_release_filters);
        let releases = db.movie_releases(id).await?;

        if let Some(ts) = api::earliest_release(&releases, filters.as_deref().unwrap_or(&default))
            && ts <= now
        {
            db.add_pending_movie(id, ts).await?;
        }
    }

    Ok(())
}

/// Recompute a movie's effective release date and pending entry from its release filters.
///
/// The effective release date is the earliest release matching the filters; it is written back to
/// `movies.release_date` so the displayed date reflects the settings (when no release matches, the
/// existing date is kept rather than cleared, e.g. when release detail could not be fetched).
/// Movies with watches keep their release date but are left to the watch flow for pending; otherwise
/// an already-released date makes the movie pending and a stale/future/no-longer-matching entry is
/// removed.
pub(crate) async fn update_movie_pending(
    db: &Database,
    movie_id: api::MovieId,
) -> anyhow::Result<()> {
    let Some(movie) = db.movie_by_id(movie_id).await? else {
        return Ok(());
    };

    let now = api::Timestamp::now();
    let default = db.load_config().await?.release_filters;
    let release = movie.pending_release(&default);

    if let Some(ts) = release
        && movie.release_date != Some(ts)
    {
        db.set_movie_release_date(movie_id, Some(ts)).await?;
    }

    if db.has_movie_watches(movie_id).await? {
        return Ok(());
    }

    match release {
        Some(ts) if ts <= now => db.add_pending_movie(movie_id, ts).await?,
        _ => db.remove_pending_movie(movie_id).await?,
    }

    Ok(())
}

pub(crate) async fn run(
    db: Database,
    queue: TaskQueue,
    broadcast: Broadcaster,
    config_changed: Arc<Notify>,
    shutdown: Shutdown,
) -> anyhow::Result<()> {
    discover_pending_movies(&db).await?;

    let mut interval = tokio::time::interval(POLL);
    let mut config = db.load_config().await?;

    if !config.auto_sync_enabled {
        info!("Background sync disabled, skipping");
    }

    loop {
        tokio::select! {
            _ = interval.tick(), if config.auto_sync_enabled => {}
            _ = config_changed.notified() => {
                config = db.load_config().await?;

                if !config.auto_sync_enabled {
                    info!("Background sync disabled, skipping");
                }

                continue;
            }
            _ = shutdown.cancelled() => { return Ok(()); }
        }

        tracing::info!("Starting background sync poll");
        discover_pending_movies(&db).await?;

        let interval_hours = config.auto_sync_interval_hours.max(1);

        let stale_show = db.shows_needing_sync(interval_hours).await?;
        let stale_movies = db.movies_needing_sync(interval_hours).await?;

        info!(
            show = stale_show.len(),
            movies = stale_movies.len(),
            interval_hours,
            "Background sync poll"
        );

        for s in stale_show {
            queue
                .push(
                    api::TaskKind::SyncShow {
                        show_id: s.id,
                        title: s.title,
                    },
                    false,
                    &broadcast,
                )
                .await;
        }

        for m in stale_movies {
            queue
                .push(
                    api::TaskKind::SyncMovie {
                        movie_id: m.id,
                        title: m.title,
                    },
                    false,
                    &broadcast,
                )
                .await;
        }
    }
}
