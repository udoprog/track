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

#[test]
fn read_only_open_neither_creates_nor_migrates() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.db");

    assert!(Database::open(&path, OpenMode::ReadOnly, 1).is_err());
    assert!(!path.exists());

    drop(Database::open(&path, OpenMode::Bulk, 1)?);
    drop(Database::open(&path, OpenMode::ReadOnly, 1)?);

    let mut ids: Vec<String> = Migrations::iter().map(|id| id.to_string()).collect();
    ids.sort();
    let last = ids.last().unwrap();

    let c = OpenOptions::new().read_write().no_mutex().open(&path)?;
    c.prepare("DELETE FROM migrations WHERE id = ?")?
        .execute((last.as_str(),))?;
    drop(c);

    let error = Database::open(&path, OpenMode::ReadOnly, 1).err().unwrap();
    assert!(error.to_string().contains(last.as_str()), "{error:#}");

    let c = OpenOptions::new().read_write().no_mutex().open(&path)?;
    let mut q = c.prepare("SELECT 1 FROM migrations WHERE id = ?")?;
    q.bind(last.as_str())?;
    assert!(
        q.next::<i64>()?.is_none(),
        "the pending migration was not applied"
    );

    Ok(())
}
/// A failed or panicking transaction leaves none of its writes behind, and the
/// write connection is usable afterwards.
#[tokio::test]
async fn transaction_rolls_back_on_error_and_panic() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.db");
    let db = Database::open(&path, OpenMode::Normal, 1)?;

    let failed = db
        .transaction(|s| -> Result<()> {
            s.set_config("first", "1")?;
            s.set_config("second", "2")?;
            anyhow::bail!("fails after both writes")
        })
        .await;

    assert!(failed.is_err());
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM config WHERE key IN ('first', 'second')"
        )?,
        0
    );

    let panicked = db
        .transaction(|s| -> Result<()> {
            s.set_config("first", "1")?;
            panic!("panics after a write")
        })
        .await;

    assert!(panicked.is_err());
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM config WHERE key = 'first'")?,
        0
    );

    db.transaction(|s| s.set_config("first", "1")).await?;
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM config WHERE key = 'first'")?,
        1
    );
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

    let root = db.default_owner().await.unwrap();
    let show = api::ShowId::new(1);
    db.create_show(show, "", None, "").await.unwrap();
    db.set_show_tracked(root, show, true).await.unwrap();

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
    db.set_show_tracked(root, show, false).await.unwrap();
    assert!(
        due(db.clone()).await.is_empty(),
        "a show nobody tracks contributes no episodes"
    );
}

/// Skipping an episode must stamp the pending row with the *next* episode's air
/// date, so an unaired successor stays dormant until it falls inside the
/// dashboard cutoff - the same rule the mark-watched path follows.
#[tokio::test]
async fn skip_pending_episode_uses_next_air_date() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1).unwrap();

    let root = db.default_owner().await.unwrap();
    let show = api::ShowId::new(1);
    db.create_show(show, "", None, "").await.unwrap();
    db.set_show_tracked(root, show, true).await.unwrap();

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
    db.add_pending_episode(root, show, e1, now).await.unwrap();

    db.skip_pending_episode(root, show, e1, now).await.unwrap();

    // E2 airs beyond the cutoff, so it is not on the dashboard yet.
    assert!(db.pending(root, now).await.unwrap().is_empty());

    // Once the cutoff reaches its air date it surfaces.
    let pending = db.pending(root, airs_later).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].timestamp, airs_later);

    // Skipping the last episode empties the show's pending slot.
    db.skip_pending_episode(root, show, e2, now).await.unwrap();
    assert!(db.pending(root, airs_later).await.unwrap().is_empty());
}

/// Person names are stored with a country (`eng-US`), and the people list must
/// still resolve them when no display language is configured.
#[tokio::test]
async fn list_persons_resolves_names_in_the_persons_language() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.db");
    let db = Database::open(&path, OpenMode::Bulk, 1)?;

    // 6148257917992593152 is `eng` with country `US`.
    let c = OpenOptions::new().read_write().no_mutex().open(&path)?;
    c.execute(
        "INSERT INTO people (id, default_language) VALUES (1, 6148257917992593152);
         INSERT INTO person_strings (person_id, language, kind, text)
         VALUES (1, 6148257917992593152, 1, 'Ada Lovelace');",
    )?;

    let root = db.default_owner().await?;
    let persons = db.list_persons(root).await?;
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].name.title(), Some("Ada Lovelace"));
    Ok(())
}

/// A database that never saved release rules uses the default ones, so movie
/// releases qualify on a fresh install.
#[tokio::test]
async fn fresh_config_has_the_default_release_rules() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1)?;

    assert_eq!(
        db.load_config().await?.release_filters,
        api::FilterRules::default_release_rules()
    );

    Ok(())
}

/// Two users share the catalog but each has their own tracking, watch history
/// and pending; deleting a user takes only their own rows with them.
#[tokio::test]
async fn tracking_watch_history_and_pending_are_per_user() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1)?;

    let root = db.default_owner().await?;
    let alice = db
        .create_user("alice", None, auth::UserRole::Regular, Timestamp::now())
        .await?
        .expect("alice is free")
        .id;

    let show = api::ShowId::new(1);
    let movie = api::MovieId::new(2);
    db.create_show(show, "Show", None, "").await?;
    db.create_movie(movie, "Movie", None, "").await?;

    let season = api::SeasonNumber::from_ordinal(1);
    let e1 = api::EpisodeId::new(11);
    let e2 = api::EpisodeId::new(12);
    let now = ms(1_700_000_000_000);
    db.upsert_episode(e1, show, season, 1, None, Some(ms(1_600_000_000_000)))
        .await?;
    db.upsert_episode(e2, show, season, 2, None, Some(ms(1_600_000_100_000)))
        .await?;

    // Only root tracks the show; only alice tracks the movie.
    db.set_show_tracked(root, show, true).await?;
    db.set_movie_tracked(alice, movie, true).await?;

    let tracked = |items: Vec<api::MediaItem>| {
        items
            .into_iter()
            .filter(|i| i.tracked)
            .map(|i| i.id)
            .collect::<HashSet<_>>()
    };

    assert_eq!(
        tracked(db.media_items(root).await?),
        HashSet::from([show.get()])
    );
    assert_eq!(
        tracked(db.media_items(alice).await?),
        HashSet::from([movie.get()])
    );
    assert!(db.show_by_id(Some(root), show).await?.unwrap().tracked);
    assert!(!db.show_by_id(Some(alice), show).await?.unwrap().tracked);
    assert!(!db.movie_by_id(Some(root), movie).await?.unwrap().tracked);

    // A sync fills pending only for the users tracking the show.
    db.fill_pending_for_show(show, now).await?;
    let pending_episodes = |pending: Vec<api::Pending>| {
        pending
            .into_iter()
            .filter_map(|p| match p.info {
                api::PendingInfo::Episode { episode_id, .. } => Some(episode_id),
                api::PendingInfo::Movie { .. } => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(pending_episodes(db.pending(root, now).await?), [e1]);
    assert!(db.pending(alice, now).await?.is_empty());

    // Alice starts tracking: her own pending starts at the first episode.
    db.set_show_tracked(alice, show, true).await?;
    db.fill_pending_for_user_show(alice, show, now).await?;
    assert_eq!(pending_episodes(db.pending(alice, now).await?), [e1]);

    // Root watches E1 and moves on; alice is unaffected.
    let episode = api::WatchedKind::Episode { show, episode: e1 };
    let watched = db
        .mark_watched(root, api::WatchedId::random(), episode, MarkTime::Now, now)
        .await?;
    crate::pending::PendingSystem::new(db.clone())
        .on_episode_watched_from(root, show, e1, now)
        .await?;

    assert_eq!(pending_episodes(db.pending(root, now).await?), [e2]);
    assert_eq!(pending_episodes(db.pending(alice, now).await?), [e1]);
    assert_eq!(db.episodes_watched(root, show).await?.len(), 1);
    assert!(db.episodes_watched(alice, show).await?.is_empty());
    assert_eq!(db.watched_for_episode(root, e1).await?.len(), 1);
    assert!(db.watched_for_episode(alice, e1).await?.is_empty());

    let counts = |episodes: Vec<api::Episode>| {
        episodes
            .into_iter()
            .map(|e| e.watched_count)
            .collect::<Vec<_>>()
    };
    assert_eq!(counts(db.episodes(root, show, season).await?), [1, 0]);
    assert_eq!(counts(db.episodes(alice, show, season).await?), [0, 0]);

    // Alice cannot remove root's watch.
    db.remove_watched(alice, watched.id).await?;
    assert_eq!(db.episodes_watched(root, show).await?.len(), 1);

    // Untracking hides the show from the user's dashboard and schedule only.
    db.set_show_tracked(alice, show, false).await?;
    assert!(db.pending(alice, now).await?.is_empty());
    assert_eq!(pending_episodes(db.pending(root, now).await?), [e2]);

    // Deleting alice removes her rows and leaves root's.
    db.delete_user(alice).await?;
    let c = OpenOptions::new()
        .read_write()
        .no_mutex()
        .open(dir.path().join("test.db"))?;
    let mut q = c.prepare(
        "SELECT (SELECT COUNT(*) FROM user_tracked_movies) + (SELECT COUNT(*) FROM pending WHERE user_id NOT IN (SELECT id FROM users))",
    )?;
    assert_eq!(q.next::<i64>()?, Some(0));
    assert_eq!(db.episodes_watched(root, show).await?.len(), 1);
    Ok(())
}

fn count(path: &Path, sql: &str) -> Result<i64> {
    let c = OpenOptions::new().read_write().no_mutex().open(path)?;
    let mut q = c.prepare(sql)?;
    Ok(q.next::<i64>()?.unwrap_or_default())
}

/// Preferences without rows read as the default, only non-default values are
/// stored, and rows that do not read are skipped.
#[tokio::test]
async fn preferences_default_on_missing_rows() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.db");
    let db = Database::open(&path, OpenMode::Bulk, 1)?;
    let root = db.default_owner().await?;

    assert_eq!(
        db.load_preferences(root).await?,
        api::Preferences::default()
    );

    let preferences = api::Preferences {
        schedule_weeks: 2,
        language: api::Locale::new(api::Language::ENG, api::Country::DEFAULT),
        ..api::Preferences::default()
    };
    db.save_preferences(root, &preferences).await?;
    assert_eq!(db.load_preferences(root).await?, preferences);
    assert_eq!(count(&path, "SELECT COUNT(*) FROM user_config")?, 2);

    let c = OpenOptions::new().read_write().no_mutex().open(&path)?;
    c.execute(
        "INSERT INTO user_config (user_id, key, value)
         SELECT id, 'no-such-key', '1' FROM users WHERE login = 'root';
         INSERT INTO user_config (user_id, key, value)
         SELECT id, 'theme', '\"purple\"' FROM users WHERE login = 'root';",
    )?;
    assert_eq!(db.load_preferences(root).await?, preferences);

    db.save_preferences(root, &api::Preferences::default())
        .await?;
    assert_eq!(count(&path, "SELECT COUNT(*) FROM user_config")?, 0);
    Ok(())
}

/// A user's language and include-specials for a show or movie apply to that
/// user alone.
#[tokio::test]
async fn show_and_movie_preferences_are_per_user() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.db");
    let db = Database::open(&path, OpenMode::Bulk, 1)?;

    let root = db.default_owner().await?;
    let alice = db
        .create_user("alice", None, auth::UserRole::Regular, Timestamp::now())
        .await?
        .expect("alice is free")
        .id;

    let show = api::ShowId::new(1);
    let movie = api::MovieId::new(2);
    db.create_show(show, "Show", None, "").await?;
    db.create_movie(movie, "Movie", None, "").await?;

    let swedish = api::Locale::from_iso("sv").unwrap();
    let english = api::Locale::new(api::Language::ENG, api::Country::DEFAULT);

    db.set_show_language(root, show, swedish).await?;
    db.set_show_include_specials(root, show, IncludeSpecials::Include)
        .await?;
    db.set_movie_language(alice, movie, swedish).await?;

    let root_show = db.show_by_id(Some(root), show).await?.unwrap();
    assert_eq!(root_show.language, swedish);
    assert_eq!(root_show.include_specials, IncludeSpecials::Include);
    assert_eq!(root_show.strings.locale(), swedish);

    let alice_show = db.show_by_id(Some(alice), show).await?.unwrap();
    assert_eq!(alice_show.language, api::Locale::DEFAULT);
    assert_eq!(alice_show.include_specials, IncludeSpecials::Default);

    let shared = db.show_by_id(None, show).await?.unwrap();
    assert_eq!(shared.language, api::Locale::DEFAULT);

    assert_eq!(
        db.movie_by_id(Some(alice), movie).await?.unwrap().language,
        swedish
    );
    assert_eq!(
        db.movie_by_id(Some(root), movie).await?.unwrap().language,
        api::Locale::DEFAULT
    );

    // A user's own language applies where they picked none for the show.
    db.save_preferences(
        alice,
        &api::Preferences {
            language: english,
            ..api::Preferences::default()
        },
    )
    .await?;
    let alice_show = db.show_by_id(Some(alice), show).await?.unwrap();
    assert_eq!(alice_show.language, api::Locale::DEFAULT);
    assert_eq!(alice_show.strings.locale(), english);

    // Sync covers the languages of everyone who tracks the show or picked one.
    assert_eq!(db.show_viewer_languages(show).await?, [swedish]);
    db.set_show_tracked(alice, show, true).await?;
    assert_eq!(db.show_viewer_languages(show).await?, [swedish, english]);

    // The default removes the row.
    db.set_show_language(root, show, api::Locale::DEFAULT)
        .await?;
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM user_show_config WHERE key = 'language'"
        )?,
        0
    );
    Ok(())
}

const REMAINING_SHOW: api::ShowId = api::ShowId::new(1);

/// The show for [`mark_watched_remaining_advances_pending_like_individual_marks`]:
/// three episodes in season 1 and two in season 2.
fn remaining_episodes() -> [(api::EpisodeId, api::SeasonNumber, u32); 5] {
    let s1 = api::SeasonNumber::from_ordinal(1);
    let s2 = api::SeasonNumber::from_ordinal(2);

    [
        (api::EpisodeId::new(11), s1, 1),
        (api::EpisodeId::new(12), s1, 2),
        (api::EpisodeId::new(13), s1, 3),
        (api::EpisodeId::new(21), s2, 1),
        (api::EpisodeId::new(22), s2, 2),
    ]
}

/// With S01E01 watched and S01E02 pending, finish `season` in bulk or one
/// episode at a time as the websocket handlers do, and return the watched
/// episodes and what is pending.
async fn finish_season(
    season: api::SeasonNumber,
    bulk: bool,
) -> Result<(Vec<(api::SeasonNumber, u32)>, api::PendingBefore)> {
    let show = REMAINING_SHOW;
    let episodes = remaining_episodes();
    let now = ms(1_800_000_000_000);

    let dir = tempfile::tempdir()?;
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1)?;
    let pending = crate::pending::PendingSystem::new(db.clone());

    let root = db.default_owner().await?;
    db.create_show(show, "", None, "").await?;
    db.set_show_tracked(root, show, true).await?;

    for (n, &(id, season, number)) in episodes.iter().enumerate() {
        let aired = ms(1_700_000_000_000 + n as i64 * 604_800_000);
        db.upsert_episode(id, show, season, number, None, Some(aired))
            .await?;
    }

    let kind = |episode| api::WatchedKind::Episode { show, episode };

    db.mark_watched(
        root,
        WatchedId::random(),
        kind(episodes[0].0),
        MarkTime::Now,
        now,
    )
    .await?;
    db.add_pending_episode(root, show, episodes[1].0, now)
        .await?;

    if bulk {
        if let Some(last) = db
            .mark_watched_remaining(root, show, season, MarkTime::Now, now)
            .await?
        {
            pending
                .on_episode_watched_from(root, show, last, now)
                .await?;
        }
    } else {
        let watched = db.episodes_watched(root, show).await?;

        for &(id, s, _) in &episodes {
            if s != season || watched.iter().any(|w| w.episode_id == id) {
                continue;
            }

            db.mark_watched(root, WatchedId::random(), kind(id), MarkTime::Now, now)
                .await?;
            pending.on_episode_watched_from(root, show, id, now).await?;
        }
    }

    let mut watched = db
        .episodes_watched(root, show)
        .await?
        .into_iter()
        .map(|w| (w.season, w.number))
        .collect::<Vec<_>>();
    watched.sort();

    let pending = db.pending_before(root, kind(episodes[0].0)).await?;
    Ok((watched, pending))
}

/// Marking the rest of a season watched leaves the same watches and pending
/// episode as marking those episodes watched one by one, in order.
#[tokio::test]
async fn mark_watched_remaining_advances_pending_like_individual_marks() -> Result<()> {
    let episodes = remaining_episodes();
    let s1 = api::SeasonNumber::from_ordinal(1);
    let s2 = api::SeasonNumber::from_ordinal(2);

    // Across the season boundary: S01E02 and S01E03 get watched, S02E01 is next.
    let bulk = finish_season(s1, true).await?;
    assert_eq!(bulk, finish_season(s1, false).await?);
    assert_eq!(bulk.0, [(s1, 1), (s1, 2), (s1, 3)]);
    assert!(
        matches!(bulk.1, api::PendingBefore::Episode { episode, .. } if episode == episodes[3].0),
        "pending should advance to S02E01, got {:?}",
        bulk.1
    );

    // Finishing the last season leaves no next episode to queue.
    let bulk = finish_season(s2, true).await?;
    assert_eq!(bulk, finish_season(s2, false).await?);
    assert_eq!(bulk.0, [(s1, 1), (s2, 1), (s2, 2)]);
    assert_eq!(bulk.1, api::PendingBefore::None);

    Ok(())
}

/// The next episode in watch order follows the most recent watch in its scope,
/// so a rewatch continues in order, and the specials are kept apart from the
/// regular seasons.
#[tokio::test]
async fn next_episode_follows_the_most_recent_watch_in_scope() -> Result<()> {
    use api::EpisodeScope::{Regular, Specials};

    let dir = tempfile::tempdir()?;
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1)?;

    let root = db.default_owner().await?;
    let show = api::ShowId::new(1);
    db.create_show(show, "", None, "").await?;

    let now = ms(1_800_000_000_000);
    let aired = ms(1_700_000_000_000);
    let unaired = ms(1_900_000_000_000);

    let s0 = api::SeasonNumber::Specials;
    let s1 = api::SeasonNumber::from_ordinal(1);
    let s2 = api::SeasonNumber::from_ordinal(2);

    let sp1 = api::EpisodeId::new(1);
    let sp2 = api::EpisodeId::new(2);
    let e1 = api::EpisodeId::new(11);
    let e2 = api::EpisodeId::new(12);
    let e3 = api::EpisodeId::new(13);
    let e4 = api::EpisodeId::new(21);

    for (id, season, number, aired) in [
        (sp1, s0, 1, aired),
        (sp2, s0, 2, aired),
        (e1, s1, 1, aired),
        (e2, s1, 2, aired),
        (e3, s1, 3, aired),
        (e4, s2, 1, unaired),
    ] {
        db.upsert_episode(id, show, season, number, None, Some(aired))
            .await?;
    }

    let next = |scope| db.next_episode(root, show, scope, now);
    let listed = async || -> Result<(bool, bool)> {
        let item = db
            .media_items(root)
            .await?
            .into_iter()
            .find(|m| m.kind == api::MediaKind::Shows && m.id == show.get())
            .context("show is listed")?;
        Ok((item.next_regular, item.next_specials))
    };
    let mut minute = 0;
    let mut watch = async |season, number| {
        minute += 1;
        db.insert_watched_episode(
            root,
            WatchedId::random(),
            ms(1_750_000_000_000 + minute * 60_000),
            show,
            season,
            number,
        )
        .await
    };

    // No watches: the first aired episode of each scope.
    assert_eq!(next(Regular).await?, Some(e1));
    assert_eq!(next(Specials).await?, Some(sp1));
    assert_eq!(listed().await?, (true, true));

    // Mid-season: the episode after the watch; specials are untouched.
    watch(s1, 1).await?;
    assert_eq!(next(Regular).await?, Some(e2));
    assert_eq!(next(Specials).await?, Some(sp1));

    // The next episode after S01E03 has not aired yet.
    watch(s1, 3).await?;
    assert_eq!(next(Regular).await?, None);

    // A rewatch of an earlier episode continues from it, even into an
    // episode already watched.
    watch(s1, 2).await?;
    assert_eq!(next(Regular).await?, Some(e3));

    // Watching specials leaves the regular scope alone, and the last special
    // ends that scope.
    watch(s0, 1).await?;
    assert_eq!(next(Specials).await?, Some(sp2));
    assert_eq!(next(Regular).await?, Some(e3));
    watch(s0, 2).await?;
    assert_eq!(next(Specials).await?, None);
    assert_eq!(next(Regular).await?, Some(e3));
    assert_eq!(listed().await?, (true, false));

    // Once S02E01 airs, it follows the end of season 1.
    watch(s1, 3).await?;
    assert_eq!(next(Regular).await?, None);
    assert_eq!(
        db.next_episode(root, show, Regular, unaired).await?,
        Some(e4)
    );

    assert_eq!(listed().await?, (false, false));

    Ok(())
}

/// The base schema of databases created before users, frozen.
const BEFORE_USERS_SCHEMA: &str = include_str!("testdata/2026-06-05-before-users.sql");

/// One text column of every row `sql` returns.
fn texts(c: &sqll::Connection, sql: &str) -> Result<Vec<String>> {
    let mut q = c.prepare(sql)?;
    let mut out = Vec::new();

    while let Some(text) = q.next::<String>()? {
        out.push(text);
    }

    Ok(out)
}

/// Tables, columns, indexes, foreign keys and normalized SQL of a schema, one
/// line each, ignoring the order objects were created in.
fn schema(c: &sqll::Connection) -> Result<Vec<String>> {
    const OBJECTS: &str =
        "SELECT name, type, tbl_name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'";

    let mut out = Vec::new();

    out.extend(texts(
        c,
        &format!(
            "SELECT m.name || '.' || p.cid || ' ' || p.name || ' ' || p.type || ' notnull=' || p.\"notnull\" || ' default=' || coalesce(p.dflt_value, 'NULL') || ' pk=' || p.pk
            FROM ({OBJECTS}) m JOIN pragma_table_info(m.name) p WHERE m.type = 'table'"
        ),
    )?);

    out.extend(texts(
        c,
        &format!(
            "SELECT m.name || ' index ' || l.name || ' unique=' || l.\"unique\" || ' partial=' || l.partial || ' (' || (SELECT group_concat(coalesce(i.name, '<expr>'), ', ' ORDER BY i.seqno) FROM pragma_index_info(l.name) i) || ')'
            FROM ({OBJECTS}) m JOIN pragma_index_list(m.name) l WHERE m.type = 'table'"
        ),
    )?);

    out.extend(texts(
        c,
        &format!(
            "SELECT m.name || ' fk ' || f.\"from\" || ' -> ' || f.\"table\" || '.' || coalesce(f.\"to\", '<pk>') || ' on delete ' || f.on_delete
            FROM ({OBJECTS}) m JOIN pragma_foreign_key_list(m.name) f WHERE m.type = 'table'"
        ),
    )?);

    // Renames and dropped columns rewrite stored SQL, so compare it with
    // quoting and whitespace removed.
    out.extend(texts(
        c,
        &format!(
            "SELECT type || ' ' || name || ' on ' || tbl_name || ': ' || coalesce(sql, '') FROM ({OBJECTS})"
        ),
    )?
    .into_iter()
    .map(|line| {
        line.replace('"', "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace("( ", "(")
            .replace(" )", ")")
            .replace(" ,", ",")
    }));

    out.sort();
    Ok(out)
}

/// A database created before users, with its base schema recorded as applied.
fn before_users_db(path: &Path) -> Result<sqll::Connection> {
    let c = OpenOptions::new()
        .read_write()
        .create()
        .no_mutex()
        .open(path)?;

    c.execute(BEFORE_USERS_SCHEMA)?;
    c.execute(MIGRATIONS_INIT)?;
    c.execute(
        "INSERT INTO migrations (id, applied_at) VALUES ('2026-06-05.sql', '2026-06-05T00:00:00Z')",
    )?;
    Ok(c)
}

/// A migration that fails partway leaves none of its changes and is not
/// recorded, so it applies cleanly once the cause is gone; the migrations
/// before it stay applied.
#[test]
fn failed_migration_is_rolled_back() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.db");

    // Makes 2026-10-02-user-preferences.sql fail at its second CREATE TABLE.
    before_users_db(&path)?.execute("CREATE TABLE user_show_config (x)")?;

    assert!(Database::open(&path, OpenMode::Normal, 1).is_err());

    let c = OpenOptions::new().read_write().no_mutex().open(&path)?;

    assert_eq!(
        texts(&c, "SELECT id FROM migrations ORDER BY id")?,
        [
            "2026-06-05.sql",
            "2026-10-01-users.sql",
            "2026-10-01-watch-history-per-user.sql"
        ]
    );
    assert_eq!(
        texts(
            &c,
            "SELECT name FROM sqlite_master WHERE name = 'user_config'"
        )?,
        Vec::<String>::new()
    );
    assert_eq!(
        texts(
            &c,
            "SELECT name FROM pragma_table_info('shows') WHERE name = 'language'"
        )?,
        ["language"]
    );

    c.execute("DROP TABLE user_show_config")?;
    drop(c);

    drop(Database::open(&path, OpenMode::Normal, 1)?);
    Ok(())
}

/// An empty database whose base schema fails to apply is left empty, so the
/// next open builds it from the base again.
#[test]
fn failed_base_schema_leaves_the_database_empty() -> Result<()> {
    let c = OpenOptions::new()
        .read_write()
        .create()
        .no_mutex()
        .open_in_memory()?;

    // A view is not a table, so the database still reads as empty, but it
    // takes a name the base schema creates.
    c.execute("CREATE VIEW shows AS SELECT 1")?;

    assert!(do_migrations(&c).is_err());
    assert_eq!(
        texts(&c, "SELECT name FROM sqlite_master WHERE type = 'table'")?,
        Vec::<String>::new()
    );

    c.execute("DROP VIEW shows")?;
    do_migrations(&c)?;
    Ok(())
}

/// A database created before users migrates to exactly the schema a fresh one
/// gets, with its tracking, watch history, pending and preferences handed to
/// root.
#[test]
fn migrations_convert_a_database_from_before_users() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let old = dir.path().join("old.db");
    let fresh = dir.path().join("fresh.db");

    let en_us = api::Locale::from_iso("en-US").context("en-US")?;
    let sv = api::Locale::from_iso("sv").context("sv")?;

    {
        let c = before_users_db(&old)?;

        c.execute(format!(
            "INSERT INTO shows (id, tracked, language, include_specials) VALUES
                (1, 1, {en_us}, 1),
                (2, 0, 0, NULL);
            INSERT INTO movies (id, tracked, language) VALUES
                (10, 1, {sv}),
                (11, 0, 0);
            INSERT INTO episodes (id, show_id, season, episode) VALUES (100, 1, 1, 1);
            INSERT INTO watched_episodes (id, timestamp, show_id, season, episode) VALUES (1000, 5, 1, 1, 1);
            INSERT INTO watched_movies (id, timestamp, movie_id) VALUES (1001, 6, 10);
            INSERT INTO pending (id, timestamp, show_id, episode_id, movie_id) VALUES
                (2000, 7, 1, 100, NULL),
                (2001, 8, NULL, NULL, 10);
            INSERT INTO config (key, value) VALUES
                ('theme', 'light'),
                ('dashboard_page', '7'),
                ('dashboard_lookahead', '86400000'),
                ('schedule_weeks', 'many'),
                ('timezone', 'Europe/Stockholm'),
                ('language', 'sv'),
                ('include_specials', 'true'),
                ('tvdb_api_key', 'key');",
            en_us = en_us.to_u64(),
            sv = sv.to_u64(),
        ))?;
    }

    drop(Database::open(&old, OpenMode::Bulk, 1)?);
    drop(Database::open(&fresh, OpenMode::Bulk, 1)?);

    let old = OpenOptions::new().read_write().no_mutex().open(&old)?;
    let fresh = OpenOptions::new().read_write().no_mutex().open(&fresh)?;

    assert_eq!(schema(&old)?, schema(&fresh)?);
    assert_eq!(
        texts(&old, "SELECT id FROM migrations ORDER BY id")?,
        texts(&fresh, "SELECT id FROM migrations ORDER BY id")?
    );

    assert_eq!(
        texts(&old, "SELECT login || ' ' || role FROM users")?,
        ["root admin"]
    );
    let root = "(SELECT id FROM users WHERE login = 'root')";

    let rows = |sql: &str| texts(&old, &sql.replace("$root", root));

    assert_eq!(
        rows("SELECT CAST(show_id AS TEXT) FROM user_tracked_shows WHERE user_id = $root")?,
        ["1"]
    );
    assert_eq!(
        rows("SELECT CAST(movie_id AS TEXT) FROM user_tracked_movies WHERE user_id = $root")?,
        ["10"]
    );
    assert_eq!(
        rows(
            "SELECT id || ' ' || timestamp || ' ' || show_id || ' ' || season || ' ' || episode FROM watched_episodes WHERE user_id = $root"
        )?,
        ["1000 5 1 1 1"]
    );
    assert_eq!(
        rows(
            "SELECT id || ' ' || timestamp || ' ' || movie_id FROM watched_movies WHERE user_id = $root"
        )?,
        ["1001 6 10"]
    );
    assert_eq!(
        rows("SELECT id || ' ' || timestamp FROM pending WHERE user_id = $root ORDER BY id")?,
        ["2000 7", "2001 8"]
    );
    assert_eq!(
        rows("SELECT key || '=' || value FROM user_config WHERE user_id = $root ORDER BY key")?,
        [
            "dashboard-page=7",
            "include-specials=true",
            "language=\"sv\"",
            "theme=\"light\"",
            "timezone=\"Europe/Stockholm\"",
        ]
    );
    assert_eq!(
        rows(
            "SELECT show_id || ' ' || key FROM user_show_config WHERE user_id = $root ORDER BY show_id, key"
        )?,
        ["1 include-specials", "1 language"]
    );
    assert_eq!(
        rows("SELECT value FROM user_show_config WHERE key = 'include-specials'")?,
        ["true"]
    );

    let language = |sql: &str| -> Result<Vec<Option<api::Locale>>> {
        Ok(rows(sql)?
            .iter()
            .map(|json| api::Locale::from_json(json))
            .collect())
    };

    assert_eq!(
        language("SELECT value FROM user_show_config WHERE key = 'language'")?,
        [Some(en_us)]
    );
    assert_eq!(
        rows("SELECT movie_id || ' ' || key FROM user_movie_config WHERE user_id = $root")?,
        ["10 language"]
    );
    assert_eq!(
        language("SELECT value FROM user_movie_config WHERE key = 'language'")?,
        [Some(sv)]
    );
    assert_eq!(
        rows("SELECT key FROM config ORDER BY key")?,
        ["tvdb_api_key"]
    );
    Ok(())
}

/// A stored integer that is not a valid code is rejected instead of producing
/// a `Language` or `Country`.
#[test]
fn invalid_stored_language_and_country_are_rejected() -> Result<()> {
    let c = OpenOptions::new()
        .read_write()
        .create()
        .no_mutex()
        .open_in_memory()?;

    let read = |sql: &str| -> (
        sqll::Result<Option<api::Language>>,
        sqll::Result<Option<api::Country>>,
    ) {
        let language = c.prepare(sql).and_then(|mut q| q.next::<api::Language>());
        let country = c.prepare(sql).and_then(|mut q| q.next::<api::Country>());
        (language, country)
    };

    for value in [
        0xFFFF_FFFFi64,
        0xC328_0000,
        -1,
        1 << 32,
        0x6500_6700,
        0x4500_4700,
    ] {
        let (language, country) = read(&format!("SELECT {value}"));
        assert!(language.is_err(), "language {value:#x} was accepted");
        assert!(country.is_err(), "country {value:#x} was accepted");
    }

    let upper_eng = i64::from(u32::from_be_bytes(*b"ENG\0"));
    assert!(read(&format!("SELECT {upper_eng}")).0.is_err());
    let lower_us = i64::from(u32::from_be_bytes(*b"us\0\0"));
    assert!(read(&format!("SELECT {lower_us}")).1.is_err());

    // Well-formed codes missing from the iso tables still read.
    let zzz = i64::from(u32::from_be_bytes(*b"zzz\0"));
    let language = read(&format!("SELECT {zzz}")).0?.context("zzz")?;
    assert_eq!(language.to_raw(), *b"zzz\0");
    let zz = i64::from(u32::from_be_bytes(*b"ZZ\0\0"));
    let country = read(&format!("SELECT {zz}")).1?.context("ZZ")?;
    assert_eq!(country.to_raw(), *b"ZZ\0\0");

    let (language, country) = read("SELECT 0");
    assert_eq!(language?, Some(api::Language::DEFAULT));
    assert_eq!(country?, Some(api::Country::DEFAULT));

    let eng = i64::from(u32::from_be_bytes(api::Language::ENG.to_raw()));
    assert_eq!(read(&format!("SELECT {eng}")).0?, Some(api::Language::ENG));

    let us = i64::from(u32::from_be_bytes(api::Country::US.to_raw()));
    assert_eq!(read(&format!("SELECT {us}")).1?, Some(api::Country::US));
    Ok(())
}

#[test]
fn date_round_trips_through_yyyymmdd_integer() -> Result<()> {
    let c = OpenOptions::new()
        .read_write()
        .no_mutex()
        .open_in_memory()?;

    for (year, month, day, n) in [
        (2026, 1, 5, 20260105),
        (2024, 2, 29, 20240229),
        (2026, 12, 31, 20261231),
        (999, 7, 4, 9990704),
    ] {
        let date = api::Date::new(year, month, day).unwrap();

        let mut q = c.prepare("SELECT ?")?;
        q.bind((date,))?;
        assert_eq!(q.next::<i64>()?, Some(n));

        let mut q = c.prepare("SELECT ?")?;
        q.bind(n)?;
        assert_eq!(q.next::<api::Date>()?, Some(date));
    }

    for n in [20260230_i64, 20261301, 20260100, 0] {
        let mut q = c.prepare("SELECT ?")?;
        q.bind(n)?;
        assert!(q.next::<api::Date>().is_err(), "{n} is not a valid date");
    }

    Ok(())
}

/// The schedule covers `[start-of-day, start-of-day + days)` in the caller's timezone,
/// groups episodes per show per local day, merges movies into the same days, and only
/// includes what the user tracks.
#[tokio::test]
async fn schedule_groups_by_local_day_and_respects_window() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1)?;
    let root = db.default_owner().await?;

    let tracked = api::ShowId::new(1);
    let untracked = api::ShowId::new(2);
    db.create_show(tracked, "Tracked", None, "").await?;
    db.create_show(untracked, "Untracked", None, "").await?;
    db.set_show_tracked(root, tracked, true).await?;

    let season = api::SeasonNumber::from_ordinal(1);
    let tz = api::TimeZone::get("Asia/Tokyo").unwrap();
    let local = |s: &str| -> Timestamp {
        Timestamp::from_jiff(
            s.parse::<jiff::civil::DateTime>()
                .unwrap()
                .to_zoned(jiff::tz::TimeZone::get("Asia/Tokyo").unwrap())
                .unwrap()
                .timestamp(),
        )
    };

    // Today is 2026-06-15 in Tokyo (2026-06-14 22:00 UTC is already the 15th there).
    let now = local("2026-06-15T07:00:00");
    let info = api::TimeInfo::new(tz, now);

    let episodes = [
        // Before today's midnight: out of the window.
        (1, tracked, 1, "2026-06-14T23:59:59"),
        (2, tracked, 2, "2026-06-15T00:00:01"),
        (3, tracked, 3, "2026-06-15T00:30:00"),
        (4, tracked, 4, "2026-06-15T20:00:00"),
        (5, tracked, 5, "2026-06-16T09:00:00"),
        (6, tracked, 6, "2026-06-16T23:59:59"),
        (7, tracked, 7, "2026-06-17T00:00:01"),
        (8, untracked, 1, "2026-06-15T12:00:00"),
        // Exactly the first midnight is in the window, exactly the end midnight is not.
        (9, tracked, 8, "2026-06-15T00:00:00"),
        (10, tracked, 9, "2026-06-17T00:00:00"),
    ];

    for (id, show, number, aired) in episodes {
        db.upsert_episode(
            api::EpisodeId::new(id),
            show,
            season,
            number,
            None,
            Some(local(aired)),
        )
        .await?;
    }

    let movie = api::MovieId::new(1);
    db.create_movie(movie, "Movie", Some(local("2026-06-16T21:00:00")), "")
        .await?;
    db.set_movie_tracked(root, movie, true).await?;

    let movie_only = api::MovieId::new(2);
    db.create_movie(movie_only, "Later", Some(local("2026-06-16T22:00:00")), "")
        .await?;
    db.set_movie_tracked(root, movie_only, true).await?;

    for (id, released) in [(4, "2026-06-15T00:00:00"), (5, "2026-06-17T00:00:00")] {
        let id = api::MovieId::new(id);
        db.create_movie(id, "Edge", Some(local(released)), "")
            .await?;
        db.set_movie_tracked(root, id, true).await?;
    }

    let untracked_movie = api::MovieId::new(3);
    db.create_movie(
        untracked_movie,
        "Nope",
        Some(local("2026-06-16T10:00:00")),
        "",
    )
    .await?;

    let days = db.schedule(root, 0, 2, info.clone()).await?;

    let summary: Vec<_> = days
        .iter()
        .map(|d| {
            (
                d.date,
                d.shows
                    .iter()
                    .map(|s| {
                        (
                            s.show_id,
                            s.episodes.iter().map(|e| e.episode).collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>(),
                d.movies.iter().map(|m| m.movie_id).collect::<Vec<_>>(),
            )
        })
        .collect();

    assert_eq!(
        summary,
        vec![
            (
                api::Date::new(2026, 6, 15).unwrap(),
                vec![(tracked, vec![8, 2, 3, 4])],
                vec![api::MovieId::new(4)]
            ),
            (
                api::Date::new(2026, 6, 16).unwrap(),
                vec![(tracked, vec![5, 6])],
                vec![movie, movie_only]
            ),
        ]
    );
    assert_eq!(days[0].shows[0].show_title, "Tracked");

    // A negative offset looks backwards, and a window with no entries is empty.
    let past = db.schedule(root, -1, 1, info.clone()).await?;
    assert_eq!(past.len(), 1);
    assert_eq!(past[0].date, api::Date::new(2026, 6, 14).unwrap());
    assert_eq!(past[0].shows[0].episodes[0].episode, 1);

    assert!(db.schedule(root, 30, 3, info.clone()).await?.is_empty());
    assert!(db.schedule(root, 0, 0, info).await?.is_empty());

    Ok(())
}

/// A remote is only changed through the show that owns it.
#[tokio::test]
async fn show_remote_changes_require_the_owning_show() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1)?;

    let a = api::ShowId::new(1);
    let b = api::ShowId::new(2);
    db.create_show(a, "A", None, "").await?;
    db.create_show(b, "B", None, "").await?;
    db.add_remote(a, None, &Remote::tmdb(1)).await?;
    db.add_remote(b, None, &Remote::tmdb(2)).await?;

    let remotes = |id| {
        let db = &db;
        async move { Ok::<_, anyhow::Error>(db.show_by_id(None, id).await?.context("show")?.remotes) }
    };

    let before = remotes(b).await?;
    let foreign = before[0].id;

    db.set_remote_enabled(a, foreign, false).await?;
    db.set_remote_sync_kinds(a, foreign, Some(api::SyncKindSet::empty()))
        .await?;
    db.set_remote_cache(a, foreign, None).await?;
    db.reorder_remotes(a, vec![RemoteId::random(), foreign])
        .await?;
    db.update_remote(a, foreign, None, &Remote::tmdb(3)).await?;
    db.remove_remote(a, foreign).await?;

    assert_eq!(remotes(b).await?, before);

    db.remove_remote(b, foreign).await?;
    assert!(remotes(b).await?.is_empty());
    assert_eq!(remotes(a).await?.len(), 1);
    Ok(())
}

/// XEM, AniDB and scene remotes keep their values and a show can hold several
/// AniDB remotes (one per cour).
#[tokio::test]
async fn xem_anidb_scene_remotes_round_trip() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1)?;

    let show = api::ShowId::new(1);
    db.create_show(show, "Frieren", None, "").await?;

    let added = [
        Remote::new(RemoteSource::Anidb, api::RemoteValue::Int(17617)),
        Remote::new(RemoteSource::Anidb, api::RemoteValue::Int(18603)),
        Remote::new(
            RemoteSource::Xem,
            api::RemoteValue::Str("tvdb/424536".into()),
        ),
        Remote::new(
            RemoteSource::Scene,
            api::RemoteValue::Str("Sousou no Frieren".into()),
        ),
        Remote::new(RemoteSource::Scene, api::RemoteValue::Str("24".into())),
    ];

    for remote in &added {
        db.add_remote(show, None, remote).await?;
    }

    let remotes = db.show_by_id(None, show).await?.context("show")?.remotes;
    let mut stored = remotes.iter().map(|r| r.remote.clone()).collect::<Vec<_>>();
    stored.sort_by_key(|r| r.to_string());
    let mut expected = added.to_vec();
    expected.sort_by_key(|r| r.to_string());
    assert_eq!(stored, expected);
    Ok(())
}

/// A show's numbering survives a round trip, and XEM's codes come back by
/// system in episode order without double episodes' second parts.
#[tokio::test]
async fn show_numbering_round_trip() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::open(dir.path().join("test.db"), OpenMode::Bulk, 1)?;

    let show = api::ShowId::new(1);
    db.create_show(show, "Frieren", None, "").await?;
    assert_eq!(
        db.show_by_id(None, show).await?.context("show")?.numbering,
        None
    );

    let numbering = api::Numbering {
        ranges: vec![api::NumberingRange {
            season: 1,
            first: 29,
            last: 38,
            system: "tvdb".to_owned(),
            target_season: 2,
            target_first: 1,
        }],
    };

    db.set_show_numbering(show, Some(numbering.clone())).await?;
    let stored = db.show_by_id(None, show).await?.context("show")?.numbering;
    assert_eq!(stored, Some(numbering));

    db.set_show_numbering(show, None).await?;
    assert_eq!(
        db.show_by_id(None, show).await?.context("show")?.numbering,
        None
    );

    let n = |system: &str, part, season, episode| crate::xem::Numbering {
        system: system.to_owned(),
        part,
        season,
        episode,
        absolute: None,
    };

    let entries = vec![
        vec![n("tvdb", 0, 2, 1), n("anidb", 0, 1, 29)],
        vec![n("tvdb", 0, 1, 2), n("tvdb", 1, 1, 3), n("anidb", 0, 1, 2)],
        vec![n("tvdb", 0, 0, 1)],
    ];

    db.transaction(move |s| s.replace_xem_episodes(show, &entries))
        .await?;

    let (episodes, systems) = db.numbering_codes(show).await?;
    assert!(episodes.is_empty());

    let systems = systems
        .into_iter()
        .map(|s| (s.system, s.episodes))
        .collect::<Vec<_>>();

    assert_eq!(
        systems,
        [
            ("anidb".to_owned(), vec![(1, 2), (1, 29)]),
            ("tvdb".to_owned(), vec![(0, 1), (1, 2), (2, 1)]),
        ]
    );
    Ok(())
}
