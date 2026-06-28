use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use musli_web::api::ChannelId;
use tokio::sync::Notify;
use tracing::info;

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;

const POLL: Duration = Duration::from_secs(15 * 60);

/// Number of most-used custom languages surfaced in the LanguagePicker.
const TOP_LANGUAGES: usize = 3;

/// Recompute the most-used custom languages across shows and movies, persist them
/// to the `state` table, and broadcast so clients refresh their LanguagePicker.
pub(crate) async fn refresh_top_languages(
    db: &Database,
    broadcast: &Broadcaster,
) -> anyhow::Result<()> {
    let top_languages = db.compute_top_languages(TOP_LANGUAGES).await?;
    db.set_state_top_languages(top_languages.clone()).await?;

    broadcast.emit(
        ChannelId::NONE,
        api::AppEventKind::TopLanguagesChanged { top_languages },
        "background top languages refreshed",
    );

    Ok(())
}

pub(crate) async fn discover_pending_movies(db: &Database) -> anyhow::Result<()> {
    let now = api::Timestamp::now();
    let default = db
        .load_config()
        .await
        .context("Loading config for pending discovery")?
        .release_filters;

    let candidates = db
        .movie_pending_candidates()
        .await
        .context("Listing movie pending candidates")?;

    for (id, filters) in candidates {
        let releases = db
            .movie_releases(id)
            .await
            .with_context(|| format!("Loading releases for movie {id}"))?;

        if let Some(ts) = filters
            .as_ref()
            .unwrap_or(&default)
            .earliest_release(&releases)
            && ts <= now
        {
            db.add_pending_movie(id, ts)
                .await
                .with_context(|| format!("Adding pending entry for movie {id}"))?;
        }
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
    discover_pending_movies(&db)
        .await
        .context("Discovering pending movies at startup")?;

    queue
        .push(api::TaskKind::RefreshTopLanguages, false, &broadcast)
        .await;

    let mut interval = tokio::time::interval(POLL);
    let mut config = db.load_config().await.context("Loading initial config")?;

    if !config.auto_sync_enabled {
        info!("Background sync disabled, skipping");
    }

    loop {
        tokio::select! {
            _ = interval.tick(), if config.auto_sync_enabled => {}
            _ = config_changed.notified() => {
                config = db.load_config().await.context("Reloading config after change")?;

                if !config.auto_sync_enabled {
                    info!("Background sync disabled, skipping");
                }

                continue;
            }
            _ = shutdown.cancelled() => { return Ok(()); }
        }

        tracing::info!("Starting background sync poll");
        discover_pending_movies(&db)
            .await
            .context("Discovering pending movies")?;

        queue
            .push(api::TaskKind::RefreshTopLanguages, false, &broadcast)
            .await;

        let interval_hours = config.auto_sync_interval_hours.max(1);

        let stale_show = db
            .shows_needing_sync(interval_hours)
            .await
            .context("Listing shows needing sync")?;
        let stale_movies = db
            .movies_needing_sync(interval_hours)
            .await
            .context("Listing movies needing sync")?;

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
                        title: s.strings.title().map(str::to_owned),
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
                        title: m.strings.title().map(str::to_owned),
                    },
                    false,
                    &broadcast,
                )
                .await;
        }
    }
}
