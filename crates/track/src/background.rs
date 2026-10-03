use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use musli_web::api::ChannelId;
use tokio::sync::Notify;
use tokio::time::MissedTickBehavior;
use tracing::{error, info};

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;

const POLL: Duration = Duration::from_secs(15 * 60);

/// An episode is re-synced hourly while now is within this many hours either side of
/// its air date: remotes most often correct an episode's title, still and exact air
/// time right around broadcast, and a full show sync is far too expensive to run at
/// that cadence.
const EPISODE_AIR_WINDOW_HOURS: u32 = 24;

/// How often an episode inside its air window is re-synced. The poll runs more often
/// than this, so the cadence comes from the per-episode `last_synced_at` check rather
/// than from any timer.
const EPISODE_SYNC_INTERVAL_HOURS: u32 = 1;

/// People change rarely, so their own data is refreshed far less often than a
/// show or movie. Never-synced people (freshly discovered via credits) are always
/// picked up regardless of this interval.
const PERSON_SYNC_INTERVAL_HOURS: u32 = 24 * 30;

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

    for (user, id, filters) in candidates {
        let releases = match db.movie_releases(id).await {
            Ok(releases) => releases,
            Err(e) => {
                error!(movie = %id, "Loading releases for pending discovery: {e:#}");
                continue;
            }
        };

        if let Some(ts) = filters
            .as_ref()
            .unwrap_or(&default)
            .earliest_release(&releases)
            && ts <= now
            && let Err(e) = db.add_pending_movie(user, id, ts).await
        {
            error!(movie = %id, "Adding pending entry: {e:#}");
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
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut config = db.load_config().await.context("Loading initial config")?;

    if !config.auto_sync_enabled {
        info!("Background sync disabled, skipping");
    }

    loop {
        tokio::select! {
            _ = interval.tick(), if config.auto_sync_enabled => {}
            _ = config_changed.notified() => {
                match db.load_config().await {
                    Ok(c) => config = c,
                    Err(e) => error!("Reloading config after change: {e:#}"),
                }

                if !config.auto_sync_enabled {
                    info!("Background sync disabled, skipping");
                }

                continue;
            }
            _ = shutdown.cancelled() => { return Ok(()); }
        }

        if let Err(e) = poll(&db, &queue, &broadcast, &config).await {
            error!("Background sync poll: {e:#}");
        }
    }
}

async fn poll(
    db: &Database,
    queue: &TaskQueue,
    broadcast: &Broadcaster,
    config: &api::Config,
) -> anyhow::Result<()> {
    tracing::info!("Starting background sync poll");

    if let Err(e) = discover_pending_movies(db).await {
        error!("Discovering pending movies: {e:#}");
    }

    queue
        .push(api::TaskKind::RefreshTopLanguages, false, broadcast)
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
                broadcast,
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
                broadcast,
            )
            .await;
    }

    // Episodes around their air date, synced far more often than their show.
    let airing = db
        .episodes_needing_air_sync(EPISODE_AIR_WINDOW_HOURS, EPISODE_SYNC_INTERVAL_HOURS)
        .await
        .context("Listing episodes needing an air-window sync")?;

    info!(episodes = airing.len(), "Episode air-window sync poll");

    for (show_id, episode_id, code) in airing {
        let title = match db.show_by_id(None, show_id).await {
            Ok(show) => show.and_then(|s| s.strings.title().map(str::to_owned)),
            Err(e) => {
                error!(show = %show_id, "Loading show title for episode sync: {e:#}");
                None
            }
        };

        queue
            .push(
                api::TaskKind::SyncEpisode {
                    show_id,
                    episode_id,
                    code,
                    title,
                },
                false,
                broadcast,
            )
            .await;
    }

    // People (localized name/biography, profile images), synced independently
    // of the media they appear in - never-synced first, then the stalest.
    let stale_people = db
        .people_needing_sync(PERSON_SYNC_INTERVAL_HOURS)
        .await
        .context("Listing people needing sync")?;

    info!(people = stale_people.len(), "Person sync poll");

    for (person_id, title) in stale_people {
        queue
            .push(
                api::TaskKind::SyncPerson { person_id, title },
                false,
                broadcast,
            )
            .await;
    }

    Ok(())
}
