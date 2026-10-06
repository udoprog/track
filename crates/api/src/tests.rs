use super::*;

fn rel(source: RemoteSource, country: Country, network: &str, ts: i64) -> EpisodeRelease {
    EpisodeRelease {
        source,
        country,
        network: network.to_owned(),
        timestamp: Timestamp::from_jiff(jiff::Timestamp::from_second(ts).unwrap()),
    }
}

fn mrel(
    source: RemoteSource,
    country: Country,
    release_type: ReleaseType,
    ts: i64,
) -> MovieRelease {
    MovieRelease {
        source,
        country,
        release_type,
        timestamp: Timestamp::from_jiff(jiff::Timestamp::from_second(ts).unwrap()),
    }
}

fn rule(predicates: impl IntoIterator<Item = FilterPredicate>) -> FilterRule {
    FilterRule {
        name: String::new(),
        predicates: predicates.into_iter().collect(),
    }
}

fn rule_set(rules: impl IntoIterator<Item = FilterRule>) -> FilterRules {
    rules.into_iter().collect()
}

/// Rules that accept every release: a single rule with no predicates.
fn accept_all() -> FilterRules {
    rule_set([rule([])])
}

fn entry(source: RemoteSource, sync_kinds: Option<SyncKindSet>) -> RemoteEntry {
    RemoteEntry {
        id: RemoteId::new(1),
        slug: None,
        remote: Remote::new(source, RemoteValue::Int(1)),
        enabled: true,
        priority: 0,
        sync_kinds,
        cache: None,
    }
}

#[test]
fn sync_kind_set_serde() {
    let set = SyncKindSet::from_kinds([SyncKind::Base, SyncKind::Dates]);

    // Serializes as a sequence of snake_case strings.
    let json = serde_json::to_string(&set).unwrap();
    assert_eq!(json, r#"["base","dates"]"#);
    assert_eq!(serde_json::from_str::<SyncKindSet>(&json).unwrap(), set);

    // Deserialization also accepts the legacy integer bitmask.
    let legacy: SyncKindSet = serde_json::from_str("3").unwrap();
    assert_eq!(legacy, set);
    assert_eq!(
        serde_json::from_str::<SyncKindSet>("0").unwrap(),
        SyncKindSet::empty()
    );
}

#[test]
fn language_code_round_trip() {
    // Default sentinel.
    assert!(Language::DEFAULT.is_default());
    assert_eq!(Language::DEFAULT.to_string(), "default");
    assert_eq!(Language::DEFAULT.to_id(), None);
    assert_eq!(Language::from_iso("default"), Some(Language::DEFAULT));
    assert_eq!(Language::from_iso(""), Some(Language::DEFAULT));

    // 3-letter packs to its own bytes.
    let eng = Language::from_iso("eng").unwrap();
    assert_eq!(eng, Language::ENG);
    assert_eq!(eng.to_string(), "eng");
    assert_eq!(eng.to_id(), Some("eng"));
    assert_eq!(eng.to_part1(), Some("en"));

    // 2-letter resolves to 3-letter.
    assert_eq!(Language::from_iso("en"), Some(Language::ENG));
    assert_eq!(Language::from_iso("SV"), Language::from_iso("swe"));

    // Case-insensitive and Display/FromStr round-trip.
    let swe = Language::from_iso("Swe").unwrap();
    assert_eq!(swe.to_string().parse::<Language>().unwrap(), swe);

    // serde round-trips through the string form.
    let json = serde_json::to_string(&swe).unwrap();
    assert_eq!(json, "\"swe\"");
    assert_eq!(serde_json::from_str::<Language>(&json).unwrap(), swe);
    assert_eq!(
        serde_json::to_string(&Language::DEFAULT).unwrap(),
        "\"default\""
    );

    // Garbage is rejected.
    assert_eq!(Language::from_iso("123"), None);
    assert_eq!(Language::from_iso("toolong"), None);
}

#[test]
fn country_code_round_trip() {
    assert!(Country::DEFAULT.is_default());
    assert_eq!(Country::DEFAULT.to_string(), "default");
    assert_eq!(Country::from_iso("default"), Some(Country::DEFAULT));
    assert_eq!(Country::from_iso(""), Some(Country::DEFAULT));

    let gb = Country::from_iso("gb").unwrap();
    assert_eq!(gb, Country::GB);
    assert_eq!(gb.to_string(), "GB");
    assert_eq!(format!("{gb:?}"), "Country(GB)");
    assert_eq!(gb.to_string().parse::<Country>().unwrap(), gb);

    let json = serde_json::to_string(&gb).unwrap();
    assert_eq!(json, "\"GB\"");
    assert_eq!(serde_json::from_str::<Country>(&json).unwrap(), gb);

    assert_eq!(Country::from_iso("ZZ"), None);
    assert_eq!(Country::from_iso("USA"), None);
    assert_eq!(Country::from_iso("1A"), None);
}

#[test]
fn code_stored_values() {
    // The database stores the big-endian integer of the raw bytes.
    let stored = |raw: [u8; 4]| u32::from_be_bytes(raw);
    assert_eq!(stored(Language::DEFAULT.to_raw()), 0);
    assert_eq!(stored(Language::ENG.to_raw()), 0x656e_6700);
    assert_eq!(stored(Country::US.to_raw()), 0x5553_0000);
    assert_eq!(format!("{:?}", Language::ENG), "eng");
}

#[test]
fn expand_sync_languages_resolves_and_dedupes() {
    use std::collections::BTreeSet;

    let eng = Locale::new(Language::ENG, Country::DEFAULT);
    let fra = Locale::from_iso("fra").unwrap();
    let default = Locale::DEFAULT;

    // [DEFAULT, ENG] with a non-English original yields both locales.
    let set = expand_sync_languages(&[default, eng], fra);
    assert_eq!(set, BTreeSet::from([fra, eng]));

    // When the original is English, DEFAULT collapses onto ENG.
    let set = expand_sync_languages(&[default, eng], eng);
    assert_eq!(set, BTreeSet::from([eng]));

    // A concrete duplicate is deduped.
    let set = expand_sync_languages(&[eng, eng], fra);
    assert_eq!(set, BTreeSet::from([eng]));

    // An unresolved DEFAULT (unknown original) is dropped.
    let set = expand_sync_languages(&[default], default);
    assert!(set.is_empty());
}

#[test]
fn sync_kind_set_bits_round_trip() {
    let set = SyncKindSet::from_kinds([SyncKind::Dates]);
    assert!(set.contains(SyncKind::Dates));
    assert!(!set.contains(SyncKind::Base));
    assert_eq!(SyncKindSet::from_bits(set.bits()), set);

    // Unknown bits are masked off.
    let masked = SyncKindSet::from_bits(0xFFFF_FFFF);
    assert_eq!(
        masked,
        SyncKindSet::from_kinds(SyncKind::ALL.iter().copied())
    );

    let toggled = SyncKindSet::empty()
        .with(SyncKind::Base, true)
        .with(SyncKind::Dates, true)
        .with(SyncKind::Base, false);

    assert_eq!(toggled, SyncKindSet::from_kinds([SyncKind::Dates]));
    assert_eq!(toggled.iter().collect::<Vec<_>>(), vec![SyncKind::Dates]);
}

#[test]
fn config_sync_kinds_for_clamps_to_capability() {
    // Absent source falls back to its full capability.
    let config = Config::default();
    assert_eq!(
        config.sync_kinds_for(RemoteSource::Tvmaze),
        SyncKindSet::from_kinds([SyncKind::Dates])
    );

    // A configured entry granting more than the capability is clamped.
    let config = Config {
        sync_kinds: vec![SourceSyncKinds {
            source: RemoteSource::Tvmaze,
            kinds: SyncKindSet::from_kinds(SyncKind::ALL.iter().copied()),
        }],
        ..Config::default()
    };
    assert_eq!(
        config.sync_kinds_for(RemoteSource::Tvmaze),
        SyncKindSet::from_kinds([SyncKind::Dates])
    );
}

#[test]
fn effective_remote_sync_kinds_override_beats_global() {
    let config = Config {
        sync_kinds: vec![SourceSyncKinds {
            source: RemoteSource::Tmdb,
            kinds: SyncKindSet::from_kinds([SyncKind::Dates]),
        }],
        ..Config::default()
    };

    // No override inherits the global default.
    let inherited = entry(RemoteSource::Tmdb, None);
    assert_eq!(
        effective_remote_sync_kinds(&inherited, &config),
        SyncKindSet::from_kinds([SyncKind::Dates])
    );

    // An override wins, still clamped to capability.
    let overridden = entry(
        RemoteSource::Tmdb,
        Some(SyncKindSet::from_kinds([SyncKind::Base])),
    );
    assert_eq!(
        effective_remote_sync_kinds(&overridden, &config),
        SyncKindSet::from_kinds([SyncKind::Base])
    );
}

#[test]
fn sync_kinds_capabilities() {
    use RemoteSource::*;

    // TMDB is the full base + air-date + credits source; TVDB is base + air-dates;
    // TVmaze is air-dates only; IMDb contributes nothing and no graphics.
    assert_eq!(
        Tmdb.sync_kinds(),
        &[SyncKind::Base, SyncKind::Dates, SyncKind::Credits]
    );
    assert_eq!(Tvdb.sync_kinds(), &[SyncKind::Base, SyncKind::Dates]);
    assert_eq!(Tvmaze.sync_kinds(), &[SyncKind::Dates]);
    assert_eq!(Imdb.sync_kinds(), &[]);

    assert!(Tmdb.has_graphics());
    assert!(Tvdb.has_graphics());
    assert!(!Tvmaze.has_graphics());
    assert!(!Imdb.has_graphics());

    // Base and Credits are exclusive (first source wins); air dates accumulate.
    assert!(SyncKind::Base.is_exclusive());
    assert!(!SyncKind::Dates.is_exclusive());
    assert!(SyncKind::Credits.is_exclusive());
}

#[test]
fn eligible_sync_kinds_unions_enabled_remotes() {
    let config = Config::default();

    // A remote restricted to AirDate plus one restricted to Base together make
    // both kinds eligible.
    let both = [
        entry(
            RemoteSource::Tmdb,
            Some(SyncKindSet::from_kinds([SyncKind::Dates])),
        ),
        entry(
            RemoteSource::Tvdb,
            Some(SyncKindSet::from_kinds([SyncKind::Base])),
        ),
    ];
    assert_eq!(
        eligible_sync_kinds(&both, &config),
        SyncKindSet::from_kinds([SyncKind::Base, SyncKind::Dates])
    );

    // With Base excluded from every remote, Base is no longer eligible, so its
    // derived seasons/episodes should be cleared on sync.
    let air_only = [
        entry(
            RemoteSource::Tmdb,
            Some(SyncKindSet::from_kinds([SyncKind::Dates])),
        ),
        entry(RemoteSource::Tvmaze, None),
    ];
    let eligible = eligible_sync_kinds(&air_only, &config);
    assert!(!eligible.contains(SyncKind::Base));
    assert!(eligible.contains(SyncKind::Dates));

    // A disabled remote contributes nothing.
    let mut disabled = entry(RemoteSource::Tmdb, None);
    disabled.enabled = false;
    assert!(eligible_sync_kinds(&[disabled], &config).is_empty());
}

#[test]
fn air_date_priority_prefers_higher_ranked_source() {
    let releases = [
        rel(RemoteSource::Tmdb, Country::DEFAULT, "", 200),
        rel(RemoteSource::Tvmaze, Country::DEFAULT, "", 300),
    ];
    let priority = default_air_date_priority();
    let accept = accept_all();

    // TVmaze outranks TMDB even though its date is later.
    let aired = accept.effective_aired(&releases, &priority).unwrap();
    assert_eq!(aired.inner().as_second(), 300);

    // Flip the priority and TMDB wins.
    let flipped = [RemoteSource::Tmdb, RemoteSource::Tvmaze];
    let aired = accept.effective_aired(&releases, &flipped).unwrap();
    assert_eq!(aired.inner().as_second(), 200);
}

#[test]
fn air_date_rule_restricts_country() {
    let releases = [
        rel(RemoteSource::Tvmaze, Country::US, "", 300),
        rel(RemoteSource::Tvmaze, Country::GB, "", 100),
    ];
    let priority = default_air_date_priority();
    let rules = rule_set([rule([FilterPredicate::Countries(vec![Country::GB])])]);

    // Only the GB date qualifies.
    let aired = rules.effective_aired(&releases, &priority).unwrap();
    assert_eq!(aired.inner().as_second(), 100);
}

#[test]
fn air_date_rule_predicates_and_together() {
    let releases = [
        rel(RemoteSource::Tvmaze, Country::GB, "BBC", 100),
        rel(RemoteSource::Tvmaze, Country::GB, "ITV", 200),
        rel(RemoteSource::Tvmaze, Country::US, "BBC", 50),
    ];
    let priority = default_air_date_priority();
    // A single rule requires both GB *and* the BBC network to match.
    let rules = rule_set([rule([
        FilterPredicate::Countries(vec![Country::GB]),
        FilterPredicate::Networks(vec!["BBC".to_owned()]),
    ])]);

    let aired = rules.effective_aired(&releases, &priority).unwrap();
    assert_eq!(aired.inner().as_second(), 100);
}

#[test]
fn air_date_rules_and_together() {
    let releases = [
        rel(RemoteSource::Tvmaze, Country::GB, "BBC", 300),
        rel(RemoteSource::Tvmaze, Country::GB, "ITV", 200),
        rel(RemoteSource::Tvmaze, Country::US, "BBC", 100),
    ];
    let priority = default_air_date_priority();
    // Separate rules are AND'd: a release must satisfy *every* rule, so only the
    // GB+BBC release qualifies.
    let rules = rule_set([
        rule([FilterPredicate::Countries(vec![Country::GB])]),
        rule([FilterPredicate::Networks(vec!["BBC".to_owned()])]),
    ]);

    let aired = rules.effective_aired(&releases, &priority).unwrap();
    assert_eq!(aired.inner().as_second(), 300);
}

#[test]
fn release_empty_rules_reject_all() {
    let releases = [
        mrel(RemoteSource::Tmdb, Country::US, ReleaseType::Premiere, 50),
        mrel(RemoteSource::Tmdb, Country::US, ReleaseType::Digital, 200),
    ];

    // No rules => nothing is accepted.
    let empty = FilterRules::default();
    assert!(!empty.release_accepted(&releases[0]));
    assert!(empty.earliest_release(&releases).is_none());
}

#[test]
fn release_rule_restricts_type() {
    let releases = [
        mrel(RemoteSource::Tmdb, Country::US, ReleaseType::Premiere, 50),
        mrel(RemoteSource::Tmdb, Country::US, ReleaseType::Digital, 200),
        mrel(RemoteSource::Tmdb, Country::US, ReleaseType::Physical, 300),
    ];
    let rules = rule_set([rule([FilterPredicate::ReleaseTypes(vec![
        ReleaseType::Digital,
        ReleaseType::Physical,
    ])])]);

    // The premiere is excluded; earliest accepted is the digital release.
    assert!(!rules.release_accepted(&releases[0]));
    assert_eq!(
        rules
            .earliest_release(&releases)
            .unwrap()
            .inner()
            .as_second(),
        200
    );
}

#[test]
fn air_date_ignores_ineligible_source() {
    // A source absent from the priority list (e.g. its AirDate kind is
    // excluded) does not contribute, even as the only release.
    let releases = [rel(RemoteSource::Unknown, Country::DEFAULT, "", 50)];
    assert!(
        accept_all()
            .effective_aired(&releases, &default_air_date_priority())
            .is_none()
    );
}

#[test]
fn air_date_none_when_no_eligible_source() {
    // Excluding air dates from every remote leaves no eligible source, so even
    // a stored release yields no effective date.
    let releases = [rel(RemoteSource::Tvmaze, Country::DEFAULT, "", 50)];
    assert!(accept_all().effective_aired(&releases, &[]).is_none());
}

#[test]
fn air_date_earliest_within_winning_source() {
    let releases = [
        rel(RemoteSource::Tvmaze, Country::US, "", 300),
        rel(RemoteSource::Tvmaze, Country::JP, "", 150),
        rel(RemoteSource::Tmdb, Country::DEFAULT, "", 10),
    ];
    let aired = accept_all()
        .effective_aired(&releases, &default_air_date_priority())
        .unwrap();
    // TVmaze wins by priority; earliest of its dates is used.
    assert_eq!(aired.inner().as_second(), 150);
}

#[test]
fn locale_round_trip() {
    // The all-default sentinel.
    assert!(Locale::DEFAULT.is_default());
    assert_eq!(Locale::DEFAULT.to_string(), "default");
    assert_eq!(Locale::to_u64(Locale::DEFAULT), 0);

    // `from_iso` never parses the sentinel (nor empty).
    assert_eq!(Locale::from_iso("default"), Some(Locale::DEFAULT));
    assert_eq!(Locale::from_iso(""), Some(Locale::DEFAULT));
    assert_eq!(Locale::from_iso("invalid"), None);
    assert_eq!(Locale::from_iso("inv"), None);
    assert_eq!("default".parse::<Locale>().ok(), Some(Locale::DEFAULT));

    // Language-only: prefers the part1 form on Display.
    let en = Locale::new(Language::ENG, Country::DEFAULT);
    assert_eq!(en.to_string(), "en");
    assert_eq!(Locale::from_iso("en"), Some(en));
    assert_eq!(Locale::from_iso("eng"), Some(en));
    assert!(!en.is_default());
    assert_eq!(en.language(), Language::ENG);
    assert_eq!(en.country(), Country::DEFAULT);

    // Language + country: `en-US` preferred over `eng-US`.
    let en_us = Locale::new(Language::ENG, Country::US);
    assert_eq!(en_us.to_string(), "en-US");
    assert_eq!(Locale::from_iso("en-US"), Some(en_us));
    assert_eq!(Locale::from_iso("eng-us"), Some(en_us));
    assert_eq!(en_us.country(), Country::US);

    // A country segment must name a real country.
    assert_eq!(Locale::from_iso("en-ZZ"), None);
    assert_eq!(Locale::from_iso("en-default"), None);

    // u64 round-trips both components.
    for l in [
        Locale::DEFAULT,
        en,
        en_us,
        Locale::new(Language::ENG, Country::JP),
    ] {
        assert_eq!(Locale::from_u64(l.to_u64()), l);
    }
}

#[test]
fn locale_flag() {
    // No country: falls back to the language's own default flag (eng -> US).
    assert_eq!(
        Locale::new(Language::ENG, Country::DEFAULT).flag(),
        Some("US")
    );

    // Country set: uses that country's flag, regardless of the language flag.
    assert_eq!(Locale::new(Language::ENG, Country::JP).flag(), Some("JP"));
    assert_eq!(Locale::new(Language::ENG, Country::GB).flag(), Some("GB"));

    // The all-default sentinel has no flag.
    assert_eq!(Locale::DEFAULT.flag(), None);
}

#[test]
fn locale_backwards_compatible_with_language_integer() {
    // A bare Language is stored in the low 32 bits; the high 32 bits (country)
    // are zero. Such an integer must decode to that language with no country.
    let lang = Language::ENG;
    let language_int = u32::from_be_bytes(lang_bytes(lang)) as u64;

    let locale = Locale::from_u64(language_int);
    assert_eq!(locale.language(), lang);
    assert_eq!(locale.country(), Country::DEFAULT);
    // And the locale's own integer equals the legacy language integer.
    assert_eq!(locale.to_u64(), language_int);
}

#[test]
fn locale_serde_and_default_string() {
    // Round-trips through JSON, including the "default" sentinel string.
    for (locale, repr) in [
        (Locale::DEFAULT, "\"default\""),
        (Locale::new(Language::ENG, Country::DEFAULT), "\"en\""),
        (Locale::new(Language::ENG, Country::US), "\"en-US\""),
    ] {
        assert_eq!(serde_json::to_string(&locale).unwrap(), repr);
        assert_eq!(serde_json::from_str::<Locale>(repr).unwrap(), locale);
    }

    // Legacy bare-language payloads still decode (as language-only locales).
    assert_eq!(
        serde_json::from_str::<Locale>("\"eng\"").unwrap(),
        Locale::new(Language::ENG, Country::DEFAULT)
    );
}

fn pt_br() -> Locale {
    Locale::from_iso("pt-BR").unwrap()
}

fn en() -> Locale {
    Locale::new(Language::ENG, Country::DEFAULT)
}

/// Build a [`Translations`] incrementally from `(kind, locale, text)` rows.
fn build(locale: Locale, entries: &[(StringKind, Locale, &str)]) -> Translations {
    let mut t = Translations::new(locale);
    for (kind, loc, text) in entries {
        t.insert(*kind, *loc, text);
    }
    t
}

#[test]
fn translations_exact_and_fallbacks() {
    // Configured locale en-US, default (original) language pt-BR.
    let t = build(
        Locale::EN_US,
        &[
            (StringKind::Title, en(), "English"),
            (StringKind::Title, pt_br(), "Portugues"),
            (StringKind::Overview, pt_br(), "Resumo"),
        ],
    );

    // en-US has no exact entry, but relaxes to language-only `en`.
    assert_eq!(t.title(), Some("English"));
    // Overview exists only in an unrelated language (pt-BR), so it stays missing
    // rather than substituting that locale.
    assert_eq!(t.overview(), None);
}

#[test]
fn translations_unmatched_language_is_none() {
    // Configured locale (de) has no entry and shares no language with the only
    // stored string (pt-BR), so the title is reported missing rather than
    // substituting the unrelated locale.
    let t = build(
        Locale::from_iso("de").unwrap(),
        &[(StringKind::Title, pt_br(), "Portugues")],
    );

    assert_eq!(t.title(), None);
}

#[test]
fn translations_get_with_unmatched_locale_is_none() {
    let t = build(
        Locale::DEFAULT,
        &[
            (StringKind::Title, en(), "English"),
            (
                StringKind::Title,
                Locale::from_iso("ja").unwrap(),
                "Nihongo",
            ),
        ],
    );

    // French has no exact or same-language entry, so it resolves to None rather
    // than substituting an unrelated stored title.
    assert_eq!(
        t.get_with(StringKind::Title, Locale::from_iso("fr").unwrap()),
        None,
    );
    // DEFAULT carries no language to match, so it is also None.
    assert_eq!(t.get_with(StringKind::Title, Locale::DEFAULT), None);
}

#[test]
fn translations_empty_is_none() {
    let t = Translations::new(Locale::EN_US);
    assert_eq!(t.title(), None);
    assert!(t.is_empty());
}

#[test]
fn translations_texts_lists_all_locales() {
    let t = build(
        Locale::EN_US,
        &[
            (StringKind::Title, en(), "English"),
            (StringKind::Title, pt_br(), "Portugues"),
            (StringKind::Overview, pt_br(), "Resumo"),
        ],
    );

    let mut titles: Vec<&str> = t.texts(StringKind::Title).collect();
    titles.sort_unstable();
    assert_eq!(titles, ["English", "Portugues"]);
}

#[test]
fn translations_dedup_replaces() {
    let mut t = Translations::new(Locale::EN_US);

    // Inserting the same (kind, locale) twice replaces rather than appends.
    t.insert(StringKind::Title, Locale::EN_US, "First");
    t.insert(StringKind::Title, Locale::EN_US, "Second");

    assert_eq!(t.title(), Some("Second"));
    assert_eq!(t.texts(StringKind::Title).count(), 1);

    // A different country for the same language is a distinct entry, not a dup.
    t.insert(
        StringKind::Title,
        Locale::from_iso("en-GB").unwrap(),
        "British",
    );
    assert_eq!(t.texts(StringKind::Title).count(), 2);
    // Exact lookups still distinguish the two countries.
    assert_eq!(t.get_with(StringKind::Title, Locale::EN_US), Some("Second"),);
    assert_eq!(
        t.get_with(StringKind::Title, Locale::from_iso("en-GB").unwrap(),),
        Some("British"),
    );
}

/// Helper exposing a Language's packed bytes for the backwards-compat test.
fn lang_bytes(language: Language) -> [u8; 4] {
    // Reconstructed from the public id; ENG packs as b"eng\0".
    let mut bytes = [0u8; 4];
    if let Some(id) = language.to_id() {
        for (b, o) in id.as_bytes().iter().zip(bytes.iter_mut()) {
            *o = *b;
        }
    }
    bytes
}

#[test]
fn human_duration() {
    let cases = [
        (0, "0 milliseconds"),
        (500, "500 milliseconds"),
        (1_000, "1 second"),
        (90_000, "1.5 minutes"),
        (1_800_000, "30 minutes"),
        (3_600_000, "1 hour"),
        (5_400_000, "1.5 hours"),
        (86_400_000, "1 day"),
        (129_600_000, "1.5 days"),
        (604_800_000, "1 week"),
        (1_209_600_000, "2 weeks"),
    ];

    for (millis, expected) in cases {
        assert_eq!(Duration::from_millis(millis).human().to_string(), expected);
    }
}

#[test]
fn duration_split() {
    assert_eq!(Duration::from_hours(24).split(), (1.0, DurationUnit::Day));
    assert_eq!(Duration::from_hours(12).split(), (12.0, DurationUnit::Hour));
    assert_eq!(
        Duration::from_millis(90_000).split(),
        (1.5, DurationUnit::Minute),
    );
    assert_eq!(Duration::ZERO.split(), (0.0, DurationUnit::Millisecond),);
}

#[test]
fn duration_round_trips_through_string() {
    let duration = Duration::from_millis(5_400_000);
    assert_eq!(duration.to_string(), "5400000");
    assert_eq!("5400000".parse::<Duration>().unwrap(), duration);
}

#[test]
fn timestamp_saturating_add_duration() {
    let now = Timestamp::from_jiff(jiff::Timestamp::from_second(1_700_000_000).unwrap());
    let later = now.saturating_add(Duration::from_hours(24));

    assert_eq!(
        later.inner().as_millisecond() - now.inner().as_millisecond(),
        86_400_000
    );

    // Shifting past the representable range clamps instead of panicking.
    assert!(now.saturating_add(Duration::from_millis(i64::MAX)) > now);
}

/// Every `cache` row already in the database was written before `errors` existed, so it
/// must still deserialize - otherwise `parse_remote_cache` silently drops it and every
/// remote does one needless full re-fetch. These are verbatim values from a live DB.
#[test]
fn remote_cache_without_errors_still_parses() {
    let etag: RemoteCache = serde_json::from_str(
        r#"{"etag":"W/\"41f3fb73dcc8c1b681dcc60ac18f4e53\"","kinds":["base","dates"]}"#,
    )
    .expect("legacy ETag row should parse");

    assert_eq!(
        etag.etag.as_deref(),
        Some("W/\"41f3fb73dcc8c1b681dcc60ac18f4e53\"")
    );
    assert!(etag.kinds.contains(SyncKind::Base));
    assert!(etag.errors.is_empty());

    let tvdb: RemoteCache =
        serde_json::from_str(r#"{"last_updated":"2025-12-10 22:25:33","kinds":["dates"]}"#)
            .expect("legacy lastUpdated row should parse");

    assert_eq!(tvdb.last_updated.as_deref(), Some("2025-12-10 22:25:33"));
    assert!(tvdb.errors.is_empty());
}

/// A recorded failure suppresses its sub-request until it expires, and expiry is what
/// forces the next full fetch. Both halves matter: without the first we re-probe a
/// missing entity every sync, without the second we would never retry it at all.
#[test]
fn remote_error_expires_by_kind() {
    let at = Timestamp::from_jiff(jiff::Timestamp::from_second(1_700_000_000).unwrap());

    let error = |kind| RemoteError {
        key: "episode/S02E05".to_owned(),
        message: "TVDB has no S02E05".to_owned(),
        kind,
        at,
    };

    let missing = error(RemoteErrorKind::Missing);
    let transient = error(RemoteErrorKind::Transient);

    let after = |hours: i64| at.saturating_add(Duration::from_hours(hours));

    // A transient blip is retried within the hour; a genuine absence is trusted for a day.
    assert!(transient.is_live(after(0)));
    assert!(!transient.is_live(after(2)));
    assert!(missing.is_live(after(2)));
    assert!(!missing.is_live(after(25)));

    let cache = RemoteCache {
        etag: None,
        last_updated: None,
        kinds: SyncKindSet::empty(),
        errors: vec![missing],
    };

    assert_eq!(
        cache.error("episode/S02E05").map(|e| e.kind),
        Some(RemoteErrorKind::Missing)
    );
    assert!(cache.error("episode/S01E01").is_none());

    // While live, the cache may still short-circuit (that IS the saved API call); once
    // expired it must not, or the failed sub-request would never run again.
    assert!(!cache.has_expired_errors(after(2)));
    assert!(cache.has_expired_errors(after(25)));
}

#[test]
fn preference_keys_round_trip() {
    for key in PreferenceKey::ALL {
        assert_eq!(key.as_str().parse::<PreferenceKey>(), Ok(*key));
        assert!(key.allowed_in(PreferenceScope::User));
    }

    assert_eq!(PreferenceKey::DashboardPage.as_str(), "dashboard-page");
    assert_eq!(
        "dashboard_page".parse::<PreferenceKey>(),
        Err(PreferenceError::UnknownKey)
    );
    assert!(PreferenceKey::Language.allowed_in(PreferenceScope::Movie));
    assert!(PreferenceKey::IncludeSpecials.allowed_in(PreferenceScope::Show));
    assert!(!PreferenceKey::IncludeSpecials.allowed_in(PreferenceScope::Movie));
    assert!(!PreferenceKey::Theme.allowed_in(PreferenceScope::Show));
}

#[test]
fn preferences_round_trip() {
    assert!(Preferences::default().encode().is_empty());

    let preferences = Preferences {
        theme: ThemeType::System,
        dashboard_page: 9,
        dashboard_lookahead: Duration::from_hours(3),
        schedule_weeks: 2,
        schedule_range_days: 7,
        timezone: "Europe/Stockholm".to_owned(),
        language: Locale::from_iso("sv-SE").unwrap(),
        include_specials: true,
    };

    let rows = preferences.encode();
    assert_eq!(rows.len(), PreferenceKey::ALL.len());

    let mut decoded = Preferences::default();

    for (key, json) in &rows {
        decoded.decode(key.as_str(), json).unwrap();
    }

    assert_eq!(decoded, preferences);

    let mut decoded = Preferences::default();
    assert_eq!(
        decoded.decode("theme", "\"purple\""),
        Err(PreferenceError::InvalidValue)
    );
    assert_eq!(
        decoded.decode("nope", "1"),
        Err(PreferenceError::UnknownKey)
    );
    assert_eq!(decoded, Preferences::default());
}

#[test]
fn include_specials_preference_values() {
    for value in [IncludeSpecials::Include, IncludeSpecials::Skip] {
        assert_eq!(IncludeSpecials::from_json(&value.to_json()), Some(value));
    }

    assert_eq!(IncludeSpecials::from_json("1"), None);
}

#[test]
fn viewer_languages_join_a_default_sync_language() {
    let english = Locale::new(Language::ENG, Country::DEFAULT);
    let swedish = Locale::from_iso("sv").unwrap();

    assert_eq!(
        sync_languages_for_viewers(&[Locale::DEFAULT, english], &[swedish]),
        [Locale::DEFAULT, english, swedish]
    );
    assert_eq!(
        sync_languages_for_viewers(&[english], &[swedish]),
        [english]
    );
}
