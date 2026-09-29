use super::*;

#[test]
fn migrations_apply_on_fresh_db() {
    let c = OpenOptions::new()
        .extended_result_codes()
        .read_write()
        .create()
        .no_mutex()
        .open_in_memory()
        .unwrap();

    do_migrations(&c).expect("migrations should apply");
}

#[test]
fn migrations_are_recorded_and_idempotent() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.db");

    let applied = || -> Result<Vec<String>> {
        let c = OpenOptions::new().read_write().no_mutex().open(&path)?;
        let mut q = c.prepare("SELECT id FROM migrations ORDER BY id")?;
        let mut ids = Vec::new();
        while let Some(id) = q.next::<String>()? {
            ids.push(id);
        }
        Ok(ids)
    };

    let mut embedded: Vec<String> = Migrations::iter().map(|id| id.to_string()).collect();
    embedded.sort();
    assert!(!embedded.is_empty(), "there is at least one migration");

    drop(Database::open(&path, OpenMode::Bulk, 1)?);
    assert_eq!(applied()?, embedded);

    drop(Database::open(&path, OpenMode::Bulk, 1)?);
    assert_eq!(applied()?, embedded);

    Ok(())
}
fn ms(millis: i64) -> Timestamp {
    Timestamp::from_jiff(jiff::Timestamp::from_millisecond(millis).unwrap())
}

/// The air-window query drives the hourly per-episode sync: an episode qualifies
/// only while it sits inside the window around its air date, and only once its own
/// `last_synced_at` has aged past the interval. It is scoped by the same per-show
/// flags the show-level stale query honors.
#[tokio::test]
async fn episodes_needing_air_sync_respects_window_and_interval() {
    const WINDOW_HOURS: u32 = 24;
    const INTERVAL_HOURS: u32 = 1;

    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1).unwrap();

    let now = Timestamp::now();
    let hours = |h: i64| now.saturating_add(api::Duration::from_hours(h));

    let show = api::ShowId::new(1);
    db.create_show(show, "", None, "").await.unwrap();

    let season = api::SeasonNumber::from_ordinal(1);

    // Just aired, and about to air: both inside the +/-24h window.
    let recent = api::EpisodeId::new(1);
    let upcoming = api::EpisodeId::new(2);
    // Long past, far future, and undated: all outside it.
    let old = api::EpisodeId::new(3);
    let distant = api::EpisodeId::new(4);
    let undated = api::EpisodeId::new(5);

    for (id, number, aired) in [
        (recent, 1, Some(hours(-2))),
        (upcoming, 2, Some(hours(5))),
        (old, 3, Some(hours(-100))),
        (distant, 4, Some(hours(100))),
        (undated, 5, None),
    ] {
        db.upsert_episode(id, show, season, number, None, aired)
            .await
            .unwrap();
    }

    let due = |db: Database| async move {
        db.episodes_needing_air_sync(WINDOW_HOURS, INTERVAL_HOURS)
            .await
            .unwrap()
            .into_iter()
            .map(|(_, episode_id, _)| episode_id)
            .collect::<HashSet<_>>()
    };

    // Never-synced episodes inside the window are due immediately.
    assert_eq!(
        due(db.clone()).await,
        HashSet::from([recent, upcoming]),
        "only episodes inside the air window are due"
    );

    // A sync just now puts an episode back under the interval, so it stops being due
    // until an hour has passed - this is what makes the cadence hourly despite the
    // poll running every 15 minutes.
    db.set_episode_synced_at(recent, now).await.unwrap();
    assert_eq!(due(db.clone()).await, HashSet::from([upcoming]));

    // An hour and change later it comes due again.
    db.set_episode_synced_at(recent, hours(-2)).await.unwrap();
    assert_eq!(due(db.clone()).await, HashSet::from([recent, upcoming]));

    // A show excluded from auto-sync contributes no episodes at all.
    db.set_show_auto_sync(show, false).await.unwrap();
    assert!(due(db.clone()).await.is_empty());

    db.set_show_auto_sync(show, true).await.unwrap();
    db.update_show(show, None, false).await.unwrap();
    assert!(
        due(db.clone()).await.is_empty(),
        "an untracked show contributes no episodes"
    );
}

/// Skipping an episode must stamp the pending row with the *next* episode's air
/// date, so an unaired successor stays dormant until it falls inside the
/// dashboard cutoff - the same rule the mark-watched path follows.
#[tokio::test]
async fn skip_pending_episode_uses_next_air_date() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("test.db"), OpenMode::Bulk, 1).unwrap();

    let show = api::ShowId::new(1);
    db.create_show(show, "", None, "").await.unwrap();

    let season = api::SeasonNumber::from_ordinal(1);
    let e1 = api::EpisodeId::new(1);
    let e2 = api::EpisodeId::new(2);

    let now = ms(1_700_000_000_000);
    let aired = ms(1_699_000_000_000);
    let airs_later = ms(1_800_000_000_000);

    db.upsert_episode(e1, show, season, 1, None, Some(aired))
        .await
        .unwrap();
    db.upsert_episode(e2, show, season, 2, None, Some(airs_later))
        .await
        .unwrap();
    db.add_pending_episode(show, e1, now).await.unwrap();

    db.skip_pending_episode(show, e1, now).await.unwrap();

    // E2 airs beyond the cutoff, so it is not on the dashboard yet.
    assert!(db.pending(now).await.unwrap().is_empty());

    // Once the cutoff reaches its air date it surfaces.
    let pending = db.pending(airs_later).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].timestamp, airs_later);

    // Skipping the last episode empties the show's pending slot.
    db.skip_pending_episode(show, e2, now).await.unwrap();
    assert!(db.pending(airs_later).await.unwrap().is_empty());
}
