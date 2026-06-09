use std::sync::Arc;
use std::time::Duration;

use db::Database;
use tokio::sync::Notify;
use tracing::info;

use crate::app_broadcast::Broadcaster;
use crate::task_queue::TaskQueue;

const POLL: Duration = Duration::from_secs(15 * 60);

pub(crate) async fn discover_pending_movies(db: &Database) -> anyhow::Result<()> {
    let today = api::Date::today();
    for (id, date) in db.theatrical_movie_candidates(today).await? {
        let ts = date
            .map(|d| d.to_timestamp())
            .unwrap_or_else(api::Timestamp::now);
        db.add_pending_movie(id, ts).await?;
    }
    for (id, date) in db.digital_movie_candidates(today).await? {
        let ts = date
            .map(|d| d.to_timestamp())
            .unwrap_or_else(api::Timestamp::now);
        db.add_pending_movie(id, ts).await?;
    }
    Ok(())
}

pub(crate) async fn run(
    db: Database,
    queue: TaskQueue,
    broadcast: Broadcaster,
    config_changed: Arc<Notify>,
    shutdown: crate::shutdown::Shutdown,
) -> anyhow::Result<()> {
    discover_pending_movies(&db).await?;

    let mut interval = tokio::time::interval(POLL);

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = config_changed.notified() => {}
            _ = shutdown.cancelled() => { return Ok(()); }
        }

        let config = db.load_config().await?;

        if !config.auto_sync_enabled {
            continue;
        }

        tracing::info!("starting background sync poll");
        discover_pending_movies(&db).await?;

        let hours = config.auto_sync_interval_hours.max(1);

        let stale_series = db.series_needing_sync(hours).await?;
        let stale_movies = db.movies_needing_sync(hours).await?;

        info!(
            series = stale_series.len(),
            movies = stale_movies.len(),
            interval_hours = hours,
            "background sync poll"
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
