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
    let db = Database::open(&dir.path().join("test.db"), OpenMode::Bulk, 1).unwrap();

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
