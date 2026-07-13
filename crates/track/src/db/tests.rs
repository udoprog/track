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
fn ms(millis: i64) -> Timestamp {
    Timestamp::from_jiff(jiff::Timestamp::from_millisecond(millis).unwrap())
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
