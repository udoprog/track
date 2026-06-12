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

    for (id, ts) in db.theatrical_movie_candidates(now).await? {
        let ts = ts.unwrap_or_else(api::Timestamp::now);

        db.add_pending_movie(id, ts).await?;
    }

    for (id, ts) in db.digital_movie_candidates(now).await? {
        let ts = ts.unwrap_or_else(api::Timestamp::now);

        db.add_pending_movie(id, ts).await?;
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

        let stale_series = db.series_needing_sync(interval_hours).await?;
        let stale_movies = db.movies_needing_sync(interval_hours).await?;

        info!(
            series = stale_series.len(),
            movies = stale_movies.len(),
            interval_hours,
            "Background sync poll"
        );

        for s in stale_series {
            queue
                .push(
                    api::TaskKind::SyncSeries {
                        series_id: s.id,
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
