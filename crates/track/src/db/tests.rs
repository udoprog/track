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
fn oneshot_applies_on_existing_db() {
    let c = OpenOptions::new()
        .extended_result_codes()
        .read_write()
        .create()
        .no_mutex()
        .open_in_memory()
        .unwrap();

    c.execute(MIGRATIONS_INIT).unwrap();

    // Simulate an existing database: the base schema is present (anchored by
    // `shows`) in its *old* shape - shows/movies/seasons/episodes still carry the
    // base title/name/overview columns, the `*_strings` tables exist, and the
    // release tables still carry their old surrogate `id` with rows in them.
    c.execute(
        "
        CREATE TABLE shows (
            id INTEGER PRIMARY KEY,
            title TEXT,
            overview TEXT,
            language INTEGER NOT NULL DEFAULT 0,
            default_language INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE movies (
            id INTEGER PRIMARY KEY,
            title TEXT,
            overview TEXT,
            language INTEGER NOT NULL DEFAULT 0,
            default_language INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE seasons (
            id INTEGER PRIMARY KEY,
            show_id INTEGER NOT NULL,
            name TEXT,
            overview TEXT
        );
        CREATE TABLE episodes (
            id INTEGER PRIMARY KEY,
            show_id INTEGER NOT NULL,
            name TEXT,
            overview TEXT
        );
        CREATE TABLE show_strings (
            id INTEGER PRIMARY KEY,
            show_id INTEGER NOT NULL,
            language INTEGER NOT NULL,
            kind INTEGER NOT NULL,
            text TEXT NOT NULL,
            UNIQUE (show_id, language, kind)
        );
        CREATE TABLE movie_strings (
            id INTEGER PRIMARY KEY,
            movie_id INTEGER NOT NULL,
            language INTEGER NOT NULL,
            kind INTEGER NOT NULL,
            text TEXT NOT NULL,
            UNIQUE (movie_id, language, kind)
        );
        CREATE TABLE season_strings (
            id INTEGER PRIMARY KEY,
            season_id INTEGER NOT NULL,
            language INTEGER NOT NULL,
            kind INTEGER NOT NULL,
            text TEXT NOT NULL,
            UNIQUE (season_id, language, kind)
        );
        CREATE TABLE episode_strings (
            id INTEGER PRIMARY KEY,
            episode_id INTEGER NOT NULL,
            language INTEGER NOT NULL,
            kind INTEGER NOT NULL,
            text TEXT NOT NULL,
            UNIQUE (episode_id, language, kind)
        );

        INSERT INTO shows (id, title, overview) VALUES (100, 'Old Show', 'Old overview');
        INSERT INTO seasons (id, show_id, name) VALUES (200, 100, 'Old Season');
        INSERT INTO episodes (id, show_id, name) VALUES (300, 100, 'Old Episode');

        CREATE TABLE episode_releases (
            id INTEGER PRIMARY KEY,
            episode_id INTEGER NOT NULL,
            source INTEGER NOT NULL,
            country INTEGER NOT NULL DEFAULT 0,
            network TEXT NOT NULL DEFAULT '',
            timestamp INTEGER NOT NULL,
            UNIQUE (episode_id, source, country, network)
        );

        CREATE TABLE movie_releases (
            id INTEGER PRIMARY KEY,
            movie_id INTEGER NOT NULL,
            country INTEGER NOT NULL DEFAULT 0,
            release_type INTEGER NOT NULL,
            timestamp INTEGER NOT NULL,
            UNIQUE (movie_id, country, release_type)
        );

        INSERT INTO episode_releases (id, episode_id, source, country, network, timestamp)
        VALUES (1, 10, 1, 0, 'NBC', 1234);

        INSERT INTO movie_releases (id, movie_id, country, release_type, timestamp)
        VALUES (1, 20, 0, 2, 5678);

        CREATE TABLE show_remotes (
            id INTEGER PRIMARY KEY,
            show_id INTEGER NOT NULL,
            source INTEGER NOT NULL,
            value,
            slug TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            priority INTEGER NOT NULL DEFAULT 0,
            sync_kinds INTEGER,
            UNIQUE (show_id, source, value)
        );

        CREATE TABLE movie_remotes (
            id INTEGER PRIMARY KEY,
            movie_id INTEGER NOT NULL,
            source INTEGER NOT NULL,
            value,
            slug TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            priority INTEGER NOT NULL DEFAULT 0,
            sync_kinds INTEGER,
            UNIQUE (movie_id, source, value)
        );

        CREATE TABLE images (
            id INTEGER PRIMARY KEY,
            kind INTEGER NOT NULL,
            source INTEGER NOT NULL,
            path TEXT NOT NULL,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            rank INTEGER NOT NULL DEFAULT 0,
            show_id INTEGER,
            movie_id INTEGER,
            episode_id INTEGER,
            season_id INTEGER
        );

        CREATE TABLE show_images (
            show_id INTEGER NOT NULL,
            kind INTEGER NOT NULL,
            image_id INTEGER NOT NULL,
            PRIMARY KEY (show_id, kind)
        );

        CREATE TABLE movie_images (
            movie_id INTEGER NOT NULL,
            kind INTEGER NOT NULL,
            image_id INTEGER NOT NULL,
            PRIMARY KEY (movie_id, kind)
        );
        ",
    )
    .unwrap();

    // Mark every non-oneshot migration (the baseline included) as already
    // applied so the run only exercises the oneshot against our hand-made
    // old-shape schema.
    {
        let mut insert = c
            .prepare("INSERT INTO migrations (id, applied_at) VALUES (?, ?)")
            .unwrap();

        for file in Migrations::iter() {
            let id = file.as_ref();

            if !id.contains("-oneshot-") {
                insert.reset().unwrap();
                insert.execute((id, "test")).unwrap();
            }
        }
    }

    do_migrations(&c).expect("oneshot should apply");

    // The pre-existing row survived the table rebuild.
    let mut network = c
        .prepare("SELECT network FROM episode_releases WHERE episode_id = 10")
        .unwrap();
    assert_eq!(network.next::<String>().unwrap().as_deref(), Some("NBC"));

    // The surrogate `id` column is gone.
    let mut cols = c
        .prepare("SELECT name FROM pragma_table_info('episode_releases')")
        .unwrap();
    let mut names = Vec::new();
    while let Some(name) = cols.next::<String>().unwrap() {
        names.push(name);
    }
    assert!(
        !names.iter().any(|n| n == "id"),
        "id column should be dropped: {names:?}"
    );

    // The oneshot is recorded so it never re-runs.
    let mut applied = c.prepare("SELECT 1 FROM migrations WHERE id = ?").unwrap();

    applied
        .bind("2026-06-21-oneshot-drop-release-ids.sql")
        .unwrap();

    assert!(applied.next::<i64>().unwrap().is_some());

    // The drop-base-strings oneshot dropped the base title/overview columns.
    let mut show_cols = c
        .prepare("SELECT name FROM pragma_table_info('shows')")
        .unwrap();
    let mut show_names = Vec::new();
    while let Some(name) = show_cols.next::<String>().unwrap() {
        show_names.push(name);
    }
    assert!(
        !show_names.iter().any(|n| n == "title" || n == "overview"),
        "base columns should be dropped: {show_names:?}"
    );

    // The add-remote-cache oneshot added the `cache` column to the remotes tables.
    for table in ["show_remotes", "movie_remotes"] {
        let mut cols = c
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap();
        let mut names = Vec::new();
        while let Some(name) = cols.next::<String>().unwrap() {
            names.push(name);
        }
        assert!(
            names.iter().any(|n| n == "cache"),
            "{table} should have a cache column: {names:?}"
        );
    }
}
