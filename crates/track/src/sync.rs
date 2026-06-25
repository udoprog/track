use core::mem;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Context as _, Result};
use api::{
    EpisodeId, Image, ImageId, ImageKey, ImageKind, RemoteSource, SeasonNumber, SyncKind,
    SyncKindSet,
};

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::remote::RemoteClients;
use crate::tmdb;

pub(crate) async fn sync_show(
    show_id: api::ShowId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
    pending: &crate::pending::PendingSystem,
) -> Result<()> {
    let show = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist")?;

    let config = db.load_config().await?;
    let language = show.language.or(config.language);

    // Ensure a TVmaze remote is stored (resolved via TVDB/IMDb) so air-date
    // enrichment participates in the layered order, as it did unconditionally
    // before. Best-effort: a failure here just means no TVmaze layer.
    if show.remote_by_source(RemoteSource::Tvmaze).is_none()
        && let Err(e) = ensure_tvmaze_remote(show_id, &show, remote, db).await
    {
        tracing::warn!("TVmaze id resolution skipped for show {show_id}: {e:#}");
    }

    // Re-read so a freshly stored TVmaze remote is included in the order.
    let show = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist")?;

    tracing::info!(show_id = %show_id, title = show.strings.title(), ?language, "Syncing show");

    // Visit enabled remotes in priority order, one layer per source (the
    // highest-priority entry of each source wins). Each layer contributes the
    // kinds it's configured for (global default, or its own override).
    let mut entries = show
        .remotes
        .iter()
        .filter(|e| e.enabled)
        .collect::<Vec<_>>();

    entries.sort_by_key(|e| e.priority);

    // The shared model every layer contributes to.
    let mut draft = ShowDraft::default();
    let mut seen = HashSet::new();

    for entry in entries {
        let source = *entry.remote.source();

        if !seen.insert(source) {
            continue;
        }

        // The kinds this remote should still contribute: exclusive kinds only
        // until a higher-priority layer took them, non-exclusive kinds always.
        let configured = api::effective_remote_sync_kinds(entry, &config);
        let kinds: SyncKindSet = configured.iter().filter(|k| draft.needs(*k)).collect();

        // Run the source if it still owes a kind, or just to accumulate graphics.
        if kinds.is_empty() && !source.has_graphics() {
            continue;
        }

        let do_base = kinds.contains(SyncKind::Base);
        let do_air_date = kinds.contains(SyncKind::AirDate);

        let result = match source {
            RemoteSource::Tmdb => match entry.remote.value().as_u32() {
                Some(tmdb_id) => {
                    tmdb_show_layer(
                        &mut draft,
                        &config,
                        &show,
                        tmdb_id,
                        do_base,
                        do_air_date,
                        remote,
                    )
                    .await
                }
                None => continue,
            },
            RemoteSource::Tvdb => match entry.remote.value().as_u32() {
                Some(tvdb_id) => {
                    tvdb_show_layer(&mut draft, &config, tvdb_id, do_base, do_air_date, remote)
                        .await
                }
                None => continue,
            },
            RemoteSource::Tvmaze => match entry.remote.value().as_u32() {
                Some(tvmaze_id) => tvmaze_layer(&mut draft, show_id, tvmaze_id, remote).await,
                None => continue,
            },
            _ => continue,
        };

        // A failing layer shouldn't abort the sync: lower-priority layers and the
        // data already collected still persist, and the kind stays unclaimed so a
        // later layer can fill it.
        if let Err(e) = result {
            tracing::warn!(?source, "Sync layer failed for show {show_id}: {e:#}");
            continue;
        }

        if do_air_date {
            draft.air_date_sources.insert(source);
        }

        for k in kinds {
            draft.provided.insert(k);
        }
    }

    // The kinds at least one enabled remote is configured to contribute. This
    // tells a deliberately-excluded kind (clear its derived data) apart from a
    // transient fetch failure (keep what's already stored), mirroring how air
    // dates use eligibility in `recompute_episode_aired_for_show`.
    let eligible = api::eligible_sync_kinds(&show.remotes, &config);

    // Base drives the show's seasons and episodes:
    //   - provided           → persist the fresh draft;
    //   - eligible, missing   → a configured Base source failed this run, so
    //                           keep the existing show rather than wiping it;
    //   - not eligible        → no enabled remote contributes Base, so the
    //                           seasons/episodes are orphaned and get cleared.
    if draft.provided.contains(SyncKind::Base) {
        persist_show_draft(show_id, &show, &draft, db, broadcast).await?;
    } else if eligible.contains(SyncKind::Base) {
        anyhow::bail!("Show has no syncable Base remote available");
    } else {
        db.prune_seasons(show_id, &HashSet::new()).await?;

        let show = db
            .show_by_id(show_id)
            .await?
            .context("Expected show to exist after clearing episodes")?;
        broadcast.broadcast_event(api::AppEventKind::ShowChanged { show });
        broadcast.broadcast_event(api::AppEventKind::SeasonsChanged {
            show_id,
            seasons: Vec::new(),
        });
    }

    // Merge all sources' air dates into the effective episodes.aired by priority,
    // then broadcast each season so clients pick up the recomputed dates.
    db.recompute_episode_aired_for_show(show_id, config.air_date_filters.clone())
        .await?;

    for season in db.seasons(show_id).await? {
        broadcast.broadcast_event(api::AppEventKind::EpisodesChanged {
            show_id,
            season: season.season,
        });
    }

    let now = api::Timestamp::now();
    let include_specials = show.effective_include_specials(config.include_specials);
    pending
        .fill_for_show(show_id, include_specials, now)
        .await?;
    db.set_show_synced_at(show_id, now).await?;
    broadcast.broadcast_event(api::AppEventKind::PendingChanged);
    tracing::info!(show_id = %show_id, "Sync complete");
    Ok(())
}

/// Resolve and store a TVmaze remote for the show via its TVDB or IMDb id, so
/// TVmaze participates in the layered sync order like any other remote.
async fn ensure_tvmaze_remote(
    show_id: api::ShowId,
    show: &api::Show,
    remote: &RemoteClients,
    db: &Database,
) -> Result<()> {
    let tvmaze_id = 'id: {
        if let Some(r) = show
            .remotes
            .iter()
            .find(|r| *r.remote.source() == RemoteSource::Tvdb)
        {
            let id: u32 = r
                .remote
                .value()
                .as_u32()
                .context("Expected a valid TVDB id")?;
            tracing::info!(tvdb_id = id, "Looking up TVmaze id via TVDB");
            break 'id remote.lookup_tvmaze_by_tvdb(id).await?;
        }

        if let Some(r) = show
            .remotes
            .iter()
            .find(|r| *r.remote.source() == RemoteSource::Imdb)
        {
            let imdb_id = r
                .remote
                .value()
                .as_str()
                .context("Expected a valid IMDB id")?;
            tracing::info!(imdb_id, "Looking up TVmaze id via IMDB");
            break 'id remote.lookup_tvmaze_by_imdb(imdb_id).await?;
        }

        tracing::info!(show_id = %show_id, "Skipping TVmaze id resolution: no TVDB or IMDB remote");
        return Ok(());
    };

    let Some(tvmaze_id) = tvmaze_id else {
        tracing::info!(show_id = %show_id, "TVmaze id not found");
        return Ok(());
    };

    // Idempotent via INSERT OR IGNORE.
    db.add_show_remote(show_id, None, &api::Remote::tvmaze(tvmaze_id))
        .await?;

    Ok(())
}

/// A show-level image accumulated during sync; graphics merge across every
/// enabled source in priority order.
struct DraftImage {
    kind: ImageKind,
    image: Image,
}

/// A season's metadata contributed by the base layer.
#[derive(Default)]
struct SeasonDraft {
    tvdb_id: Option<u32>,
    air_date: Option<api::Timestamp>,
    poster: Option<Image>,
    /// This is set by tvdb to indicate that languages which are available for
    /// names.
    translations: HashSet<String>,
}

/// An episode's metadata contributed by the base layer.
struct EpisodeDraft {
    tvdb_id: Option<u32>,
    absolute_number: Option<u32>,
    aired: Option<api::Timestamp>,
    screenshot: Option<Image>,
    /// This is set by tvdb to indicate that languages which are available for
    /// names.
    translations: HashSet<String>,
}

/// An episode air-date release accumulated from an air-date layer, keyed by
/// (season, number) so it can be attributed to a persisted episode.
struct DraftRelease {
    season: SeasonNumber,
    number: u32,
    source: RemoteSource,
    country: api::Country,
    network: String,
    timestamp: api::Timestamp,
}

/// The shared, mutable model the sync layers contribute to. Each layer reads
/// [`Self::needs`] to decide whether to do work for a kind, then appends what it
/// fetched. `provided` tracks which kinds have been contributed so an exclusive
/// kind (Base) is taken by the first source and skipped by later layers.
#[derive(Default)]
struct ShowDraft {
    provided: SyncKindSet,
    original_name: Option<String>,
    first_air_date: Option<api::Timestamp>,
    /// The show's own original language, discovered from the Base layer.
    default_language: api::Locale,
    /// The source/id of the Base provider, used to re-fetch per-language strings.
    base_remote: Option<(RemoteSource, u32)>,
    seasons: BTreeMap<SeasonNumber, SeasonDraft>,
    episodes: BTreeMap<(SeasonNumber, u32), EpisodeDraft>,
    remotes: Vec<(Option<String>, api::Remote)>,
    releases: Vec<DraftRelease>,
    /// Sources whose air-date layer ran successfully this sync. Scopes air-date
    /// pruning: a source here had every release it still reports re-upserted, so
    /// anything else stored for it is stale and removed - including when it now
    /// reports none. A source whose layer failed is absent, so its stored releases
    /// survive a transient fetch error.
    air_date_sources: HashSet<RemoteSource>,
    images: Vec<DraftImage>,
    selected: HashMap<ImageKind, ImageKey>,
    /// Per-language translated strings keyed by owner, populated for every target
    /// remote.
    show_strings: StringRows,
    season_strings: BTreeMap<SeasonNumber, StringRows>,
    episode_strings: BTreeMap<(SeasonNumber, u32), StringRows>,
    /// This is set by tvdb to indicate that languages which are available for
    /// names.
    translations: HashSet<String>,
}

/// A batch of `(locale, kind, text)` rows destined for a `*_strings` table.
type StringRows = Vec<(api::Locale, api::StringKind, String)>;

/// Append a translated string, skipping missing or blank text.
fn push_string(
    rows: &mut StringRows,
    language: api::Locale,
    kind: api::StringKind,
    text: Option<String>,
) {
    if let Some(text) = text
        && !text.trim().is_empty()
    {
        rows.push((language, kind, text));
    }
}

impl ShowDraft {
    /// Whether a source should still contribute `kind`: exclusive kinds only until
    /// the first source provides them, non-exclusive kinds always.
    fn needs(&self, kind: SyncKind) -> bool {
        !kind.is_exclusive() || !self.provided.contains(kind)
    }

    fn add_remote(&mut self, slug: Option<String>, remote: api::Remote) {
        self.remotes.push((slug, remote));
    }

    fn add_show_string(
        &mut self,
        language: api::Locale,
        kind: api::StringKind,
        text: Option<String>,
    ) {
        push_string(&mut self.show_strings, language, kind, text);
    }

    fn add_season_string(
        &mut self,
        season: SeasonNumber,
        language: api::Locale,
        kind: api::StringKind,
        text: Option<String>,
    ) {
        push_string(
            self.season_strings.entry(season).or_default(),
            language,
            kind,
            text,
        );
    }

    fn add_episode_string(
        &mut self,
        season: SeasonNumber,
        number: u32,
        language: api::Locale,
        kind: api::StringKind,
        text: Option<String>,
    ) {
        push_string(
            self.episode_strings.entry((season, number)).or_default(),
            language,
            kind,
            text,
        );
    }

    fn add_image(&mut self, kind: ImageKind, image: Image, selected: bool) {
        if selected {
            self.selected
                .entry(kind)
                .or_insert_with(|| image.key().clone());
        }

        self.images.push(DraftImage { kind, image });
    }
}

#[tracing::instrument(skip_all, fields(tmdb_id, do_base, do_air_date))]
async fn tmdb_show_layer(
    draft: &mut ShowDraft,
    config: &api::Config,
    show: &api::Show,
    tmdb_id: u32,
    do_base: bool,
    do_air_date: bool,
    remote: &RemoteClients,
) -> Result<()> {
    tracing::info!(tmdb_id, do_base, do_air_date, "Fetching TMDB show");

    let info = remote.fetch_tmdb_show(tmdb_id).await?;

    draft.original_name = info.original_name.or(draft.original_name.take());

    for r in &info.remotes {
        draft.add_remote(r.slug.clone(), r.remote.clone());
    }

    // Graphics accumulate from every source.
    for poster in &info.posters {
        let selected = info.selected_poster.as_ref() == Some(poster.key());
        draft.add_image(ImageKind::Poster, poster.clone(), selected);
    }

    for backdrop in &info.backdrops {
        let selected = info.selected_backdrop.as_ref() == Some(backdrop.key());
        draft.add_image(ImageKind::Backdrop, backdrop.clone(), selected);
    }

    if !do_base && !do_air_date {
        return Ok(());
    }

    if do_base {
        draft.first_air_date = info.first_air_date.or(show.first_air_date);
        draft.default_language = info.original_language;
        draft.base_remote = Some((RemoteSource::Tmdb, tmdb_id));
    }

    for season in &info.seasons {
        if do_base {
            let entry = draft.seasons.entry(season.number).or_default();
            entry.air_date = season.air_date;
            entry.poster = season.poster.clone().map(Image::from);
        }

        tracing::info!(tmdb_id, ?season.number, "Fetching TMDB season episodes");

        for e in remote
            .fetch_tmdb_season_episodes(tmdb_id, season.number)
            .await?
        {
            if do_base {
                draft.episodes.insert(
                    (e.season, e.number),
                    EpisodeDraft {
                        tvdb_id: None,
                        absolute_number: None,
                        aired: e.aired,
                        screenshot: e.filename.map(Image::from),
                        translations: HashSet::new(),
                    },
                );
            }

            if do_air_date && let Some(aired) = e.aired {
                draft.releases.push(DraftRelease {
                    season: e.season,
                    number: e.number,
                    source: RemoteSource::Tmdb,
                    country: api::Country::DEFAULT,
                    network: String::new(),
                    timestamp: aired,
                });
            }
        }
    }

    if do_base {
        let targets = api::expand_sync_languages(&config.sync_languages, draft.default_language);

        tracing::info!("Collecting strings");

        if let Err(error) = collect_tmdb_show_strings(draft, tmdb_id, &targets, remote).await {
            tracing::warn!("String collection failed: {error:#}");
        }
    }

    Ok(())
}

#[tracing::instrument(skip_all, fields(tvdb_id, do_base, do_air_date))]
async fn tvdb_show_layer(
    draft: &mut ShowDraft,
    config: &api::Config,
    tvdb_id: u32,
    do_base: bool,
    do_air_date: bool,
    remote: &RemoteClients,
) -> Result<()> {
    tracing::info!(tvdb_id, do_base, do_air_date, "Fetching TVDB show");

    let info = remote.fetch_tvdb_show(tvdb_id).await?;

    for r in &info.remotes {
        draft.add_remote(r.slug.clone(), r.remote.clone());
    }

    for poster in &info.poster {
        let selected = info.selected_poster.as_ref() == Some(poster.key());
        draft.add_image(ImageKind::Poster, poster.clone(), selected);
    }

    for banner in &info.banner {
        let selected = info.selected_banner.as_ref() == Some(banner.key());
        draft.add_image(ImageKind::Banner, banner.clone(), selected);
    }

    for fanart in &info.fanart {
        let selected = info.selected_fanart.as_ref() == Some(fanart.key());
        draft.add_image(ImageKind::Backdrop, fanart.clone(), selected);
    }

    if do_base {
        // TVDB has no first-air-date field; persist falls back to the existing value.
        draft.default_language = info.original_language;
        draft.base_remote = Some((RemoteSource::Tvdb, tvdb_id));
        draft
            .translations
            .extend(info.name_translations.into_iter().map(|n| n.to_lowercase()));
        draft.translations.extend(
            info.overview_translations
                .into_iter()
                .map(|n| n.to_lowercase()),
        );

        for s in info.seasons {
            let entry = draft.seasons.entry(s.number).or_default();
            entry.tvdb_id = Some(s.id);
            entry
                .translations
                .extend(s.name_translations.into_iter().map(|n| n.to_lowercase()));
            entry.translations.extend(
                s.overview_translations
                    .into_iter()
                    .map(|n| n.to_lowercase()),
            );
        }
    }

    tracing::info!(tvdb_id, "Fetching TVDB episodes");

    let episodes = remote.fetch_tvdb_episodes(tvdb_id).await?;

    tracing::info!(count = episodes.len(), "Got episodes from TVDB");

    for e in episodes {
        if do_base {
            // TVDB has no season records; derive the air date as the earliest
            // episode air date in the season.
            let entry = draft.seasons.entry(e.season).or_default();

            if let Some(aired) = e.aired {
                entry.air_date = Some(match entry.air_date {
                    Some(cur) if cur <= aired => cur,
                    _ => aired,
                });
            }

            let screenshot = e
                .image
                .as_ref()
                .map(|(source, path)| Image::new(*source, path));

            let translations = e
                .name_translations
                .into_iter()
                .chain(e.overview_translations.into_iter())
                .map(|n| n.to_lowercase())
                .collect();

            draft.episodes.insert(
                (e.season, e.number),
                EpisodeDraft {
                    tvdb_id: Some(e.id),
                    absolute_number: e.absolute_number,
                    aired: e.aired,
                    screenshot,
                    translations,
                },
            );
        }

        if do_air_date && let Some(aired) = e.aired {
            draft.releases.push(DraftRelease {
                season: e.season,
                number: e.number,
                source: RemoteSource::Tvdb,
                country: api::Country::DEFAULT,
                network: String::new(),
                timestamp: aired,
            });
        }
    }

    if do_base {
        let targets = api::expand_sync_languages(&config.sync_languages, draft.default_language);

        // TVDB has no country dimension, so its strings are language-only.
        // Collapse each target to its language (country = DEFAULT) before
        // fetching; the `BTreeSet` then dedupes locales that differ only by
        // country.
        let tvdb_targets: BTreeSet<api::Locale> = targets
            .into_iter()
            .map(|l| api::Locale::new(l.language(), api::Country::DEFAULT))
            .collect();

        for language in tvdb_targets {
            tracing::info!(?language, "Collecting strings");

            // Remotes key on ISO 639-1; skip any locale whose language has no
            // 2-letter form.
            if language.language().to_part1().is_none() {
                continue;
            }

            if let Err(error) = collect_tvdb_strings(draft, tvdb_id, language, remote).await {
                tracing::warn!("String collection failed: {error:#}");
            }
        }
    }

    Ok(())
}

#[tracing::instrument(skip_all, fields(show_id))]
async fn tvmaze_layer(
    draft: &mut ShowDraft,
    show_id: api::ShowId,
    tvmaze_id: u32,
    remote: &RemoteClients,
) -> Result<()> {
    let network = match remote.fetch_tvmaze_show_network(tvmaze_id).await {
        Ok(network) => network,
        Err(e) => {
            tracing::warn!("TVmaze network lookup failed for show {show_id}: {e:#}");
            Default::default()
        }
    };

    tracing::info!(tvmaze_id, "Fetching TVmaze episodes");

    let episodes = remote.fetch_tvmaze_episodes(tvmaze_id).await?;
    let count = episodes.len();

    for ep in episodes {
        draft.releases.push(DraftRelease {
            season: ep.season,
            number: ep.number,
            source: RemoteSource::Tvmaze,
            country: network.country,
            network: network.network.clone(),
            timestamp: ep.aired_at,
        });
    }

    tracing::info!(episodes = count, "Collected TVmaze air dates");

    Ok(())
}

/// Returns true if `locale` (from a TMDB translations response) is relevant
/// given the set of configured target languages. A language-only target
/// (country = DEFAULT) matches any country variant; an exact-country target
/// requires a full match.
fn locale_matches_targets(locale: api::Locale, targets: &BTreeSet<api::Locale>) -> bool {
    targets.iter().any(|t| {
        if t.country().is_default() {
            t.language() == locale.language()
        } else {
            *t == locale
        }
    })
}

#[tracing::instrument(skip_all, fields(tmdb_id))]
async fn collect_tmdb_show_strings(
    draft: &mut ShowDraft,
    tmdb_id: u32,
    targets: &BTreeSet<api::Locale>,
    remote: &RemoteClients,
) -> Result<()> {
    // Show strings: one translations call instead of one full-detail call per
    // language.
    let translations = remote.fetch_tmdb_show_translations(tmdb_id).await?;

    for translation in translations {
        if !locale_matches_targets(translation.locale, targets) {
            continue;
        }

        tracing::info!(?translation, ?targets, "TMDB show translation");

        draft.add_show_string(
            translation.locale,
            api::StringKind::Title,
            translation.name.or(draft.original_name.clone()),
        );

        draft.add_show_string(
            translation.locale,
            api::StringKind::Overview,
            translation.overview,
        );
    }

    // Season strings: one translations call per season instead of one
    // full-detail call per season per language.
    let season_numbers: Vec<SeasonNumber> = draft.seasons.keys().copied().collect();

    for season_number in &season_numbers {
        let translations = remote
            .fetch_tmdb_season_translations(tmdb_id, *season_number)
            .await?;

        for translation in translations {
            if !locale_matches_targets(translation.locale, targets) {
                continue;
            }

            tracing::info!(
                ?translation,
                ?targets,
                ?season_number,
                "TMDB season translation"
            );

            draft.add_season_string(
                *season_number,
                translation.locale,
                api::StringKind::Title,
                translation.name,
            );

            draft.add_season_string(
                *season_number,
                translation.locale,
                api::StringKind::Overview,
                translation.overview,
            );
        }
    }

    // Episode strings: one translations call per episode.
    let episode_keys: Vec<(SeasonNumber, u32)> = draft.episodes.keys().copied().collect();

    for (season_number, episode_number) in &episode_keys {
        let translations = remote
            .fetch_tmdb_episode_translations(tmdb_id, *season_number, *episode_number)
            .await?;

        for row in translations {
            if !locale_matches_targets(row.locale, targets) {
                continue;
            }

            draft.add_episode_string(
                *season_number,
                *episode_number,
                row.locale,
                api::StringKind::Title,
                row.name,
            );

            draft.add_episode_string(
                *season_number,
                *episode_number,
                row.locale,
                api::StringKind::Overview,
                row.overview,
            );
        }
    }

    Ok(())
}

async fn collect_tvdb_strings(
    draft: &mut ShowDraft,
    tvdb_id: u32,
    language: api::Locale,
    remote: &RemoteClients,
) -> Result<()> {
    if let Some(translation) = remote
        .fetch_tvdb_show_translation(tvdb_id, language, &draft.translations)
        .await?
    {
        tracing::info!(
            ?tvdb_id,
            ?language,
            translations = ?draft.translations,
            ?translation,
            "Fetching show translation"
        );

        draft.add_show_string(language, api::StringKind::Title, translation.name);
        draft.add_show_string(language, api::StringKind::Overview, translation.overview);
    }

    let seasons = draft
        .seasons
        .iter_mut()
        .flat_map(|(number, s)| Some((*number, s.tvdb_id?, mem::take(&mut s.translations))))
        .collect::<Vec<_>>();

    for (season, tvdb_id, translations) in seasons {
        if let Some(translation) = remote
            .fetch_tvdb_season_translation(tvdb_id, language, &translations)
            .await?
        {
            tracing::info!(
                ?tvdb_id,
                ?season,
                ?language,
                ?translation,
                "Fetching season translation"
            );

            draft.add_season_string(season, language, api::StringKind::Title, translation.name);

            draft.add_season_string(
                season,
                language,
                api::StringKind::Overview,
                translation.overview,
            );
        }
    }

    let episodes = draft
        .episodes
        .iter_mut()
        .flat_map(|(key, e)| Some((*key, e.tvdb_id?, mem::take(&mut e.translations))))
        .collect::<Vec<_>>();

    for ((season, number), tvdb_id, translations) in episodes {
        if let Some(translation) = remote
            .fetch_tvdb_episode_translation(tvdb_id, language, &translations)
            .await?
        {
            tracing::info!(
                ?tvdb_id,
                ?season,
                ?number,
                ?translations,
                ?language,
                ?translation,
                "Fetching episode translation"
            );

            draft.add_episode_string(
                season,
                number,
                language,
                api::StringKind::Title,
                translation.name,
            );

            draft.add_episode_string(
                season,
                number,
                language,
                api::StringKind::Overview,
                translation.overview,
            );
        }
    }

    Ok(())
}

/// Write the accumulated [`ShowDraft`] to the database in one pass: base
/// metadata, accumulated graphics, seasons, episodes (with stable ids), and
/// per-source air-date releases; prune anything no longer present.
async fn persist_show_draft(
    show_id: api::ShowId,
    show: &api::Show,
    draft: &ShowDraft,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    db.update_show(
        show_id,
        draft.first_air_date.or(show.first_air_date),
        show.tracked,
    )
    .await?;

    if !draft.default_language.is_default() {
        db.set_show_default_language(show_id, draft.default_language)
            .await?;
    }

    db.replace_show_strings(show_id, draft.show_strings.clone())
        .await?;

    for (slug, remote) in &draft.remotes {
        db.add_show_remote(show_id, slug.as_deref(), remote).await?;
    }

    // Graphics: replace all show images with the accumulated set, ranked in
    // source-priority (accumulation) order, selecting the highest-priority pick.
    db.clear_show_images(show_id).await?;

    let mut ranks: HashMap<ImageKind, u32> = HashMap::new();
    let mut selected_ids: HashMap<ImageKind, ImageId> = HashMap::new();

    for draft_image in &draft.images {
        let id = ImageId::random();
        let rank = ranks.entry(draft_image.kind).or_default();

        db.upsert_show_image(id, show_id, draft_image.kind, *rank, &draft_image.image)
            .await?;
        *rank += 1;

        if draft.selected.get(&draft_image.kind) == Some(draft_image.image.key()) {
            selected_ids.entry(draft_image.kind).or_insert(id);
        }
    }

    for (kind, id) in selected_ids {
        db.set_show_image_selection(show_id, kind, id).await?;
    }

    // Episodes: assign stable ids (reuse existing) so air-date releases
    // attribute to the right row.
    let existing_episode_ids = db.episode_ids(show_id).await?;

    db.clear_episode_images(show_id).await?;

    let mut episode_ids: HashMap<(SeasonNumber, u32), EpisodeId> = HashMap::new();
    let mut season_episode_numbers: HashMap<SeasonNumber, HashSet<u32>> = HashMap::new();

    for ((season, number), ep) in &draft.episodes {
        let episode_id = existing_episode_ids
            .get(&(*season, *number))
            .copied()
            .unwrap_or_else(EpisodeId::random);

        episode_ids.insert((*season, *number), episode_id);

        season_episode_numbers
            .entry(*season)
            .or_default()
            .insert(*number);

        db.upsert_episode(
            episode_id,
            show_id,
            *season,
            *number,
            ep.absolute_number,
            ep.aired,
        )
        .await?;

        let strings = draft
            .episode_strings
            .get(&(*season, *number))
            .cloned()
            .unwrap_or_default();
        db.replace_episode_strings(episode_id, strings).await?;

        if let Some(screenshot) = &ep.screenshot {
            let image_id = ImageId::random();
            db.upsert_episode_image(image_id, episode_id, ImageKind::Screenshot, screenshot)
                .await?;
            db.set_episode_image_selection(episode_id, ImageKind::Screenshot, image_id)
                .await?;
        }
    }

    // Seasons.
    let mut synced_seasons = HashSet::new();

    for (number, season) in &draft.seasons {
        let season_id = db.upsert_season(show_id, *number, season.air_date).await?;

        let strings = draft
            .season_strings
            .get(number)
            .cloned()
            .unwrap_or_default();
        db.replace_season_strings(season_id, strings).await?;

        db.clear_season_images(season_id).await?;

        if let Some(poster) = &season.poster {
            let image_id = ImageId::random();
            db.upsert_season_image(image_id, season_id, ImageKind::Poster, poster)
                .await?;
            db.set_season_image_selection(season_id, ImageKind::Poster, image_id)
                .await?;
        }

        synced_seasons.insert(*number);
    }

    for (season, kept) in &season_episode_numbers {
        db.prune_season_episodes(show_id, *season, kept).await?;
    }

    db.prune_seasons(show_id, &synced_seasons).await?;

    // Air-date releases, attributed per source; skip episodes we didn't persist.
    // Track what we wrote so stale releases can be pruned afterwards, scoped to the
    // sources whose air-date layer actually ran this sync (`draft.air_date_sources`).
    let mut kept_releases = HashSet::new();

    for r in &draft.releases {
        let Some(&episode_id) = episode_ids.get(&(r.season, r.number)) else {
            continue;
        };

        db.upsert_episode_release(episode_id, r.source, r.country, &r.network, r.timestamp)
            .await?;

        kept_releases.insert((episode_id, r.source, r.country, r.network.clone()));
    }

    db.prune_episode_releases(show_id, &kept_releases, &draft.air_date_sources)
        .await?;

    let updated = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist after update")?;
    broadcast.broadcast_event(api::AppEventKind::ShowChanged { show: updated });

    let seasons = db.seasons(show_id).await?;
    broadcast.broadcast_event(api::AppEventKind::SeasonsChanged { show_id, seasons });

    broadcast.broadcast_event(api::AppEventKind::TranslationsChanged {
        target: api::TranslationTarget::Show(show_id),
    });

    Ok(())
}

pub(crate) async fn sync_movie(
    movie_id: api::MovieId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
) -> Result<()> {
    let movie = db
        .movie_by_id(movie_id)
        .await?
        .context("Expected movie to exist")?;

    let config = db.load_config().await?;
    let language = movie.language.or(config.language);

    let source = movie.primary_sync_source();
    tracing::info!(movie_id = %movie_id, title = movie.strings.title(), ?source, ?language, "Syncing movie");

    match source {
        Some(api::RemoteSource::Tmdb) => {
            let remote_id = movie
                .remote_by_source(api::RemoteSource::Tmdb)
                .context("Expected movie to have a TMDB remote")?;

            let tmdb_id: u32 = remote_id
                .value()
                .as_u32()
                .context("Expected a valid TMDB id")?;

            tracing::info!(tmdb_id, "Fetching TMDB movie");

            let info = remote.fetch_tmdb_movie(tmdb_id).await?;

            if !info.original_language.is_default() {
                db.set_movie_default_language(movie_id, info.original_language)
                    .await?;
            }

            if let Err(e) =
                collect_tmdb_movie_strings(movie_id, tmdb_id, &info, &config, language, db, remote)
                    .await
            {
                tracing::warn!(movie_id = %movie_id, "String collection failed: {e:#}");
            }

            for remote in &info.remotes {
                db.add_movie_remote(movie_id, None, remote).await?;
            }

            db.clear_movie_images(movie_id).await?;

            let mut selected_poster_id = None;
            let mut selected_backdrop_id = None;

            for (rank, img) in info.posters.iter().enumerate() {
                let id = ImageId::random();

                db.upsert_movie_image(id, movie_id, ImageKind::Poster, rank as u32, img)
                    .await?;

                if info.selected_poster.as_ref() == Some(img.key()) {
                    selected_poster_id = Some(id);
                }
            }

            for (rank, img) in info.backdrops.iter().enumerate() {
                let id = ImageId::random();

                db.upsert_movie_image(id, movie_id, ImageKind::Backdrop, rank as u32, img)
                    .await?;

                if info.selected_backdrop.as_ref() == Some(img.key()) {
                    selected_backdrop_id = Some(id);
                }
            }

            if let Some(id) = selected_poster_id {
                db.set_movie_image_selection(movie_id, ImageKind::Poster, id)
                    .await?;
            }

            if let Some(id) = selected_backdrop_id {
                db.set_movie_image_selection(movie_id, ImageKind::Backdrop, id)
                    .await?;

                db.set_movie_image_selection(movie_id, ImageKind::Banner, id)
                    .await?;
            }

            match remote.fetch_tmdb_movie_releases(tmdb_id).await {
                Ok(releases) => {
                    tracing::info!(count = releases.len(), "Fetched TMDB movie releases");

                    let mut kept = HashSet::new();

                    for r in releases {
                        db.upsert_movie_release(
                            movie_id,
                            api::RemoteSource::Tmdb,
                            r.country,
                            r.release_type,
                            &r.release_date,
                        )
                        .await?;

                        kept.insert((api::RemoteSource::Tmdb, r.country, r.release_type));
                    }

                    db.prune_movie_releases(movie_id, &kept).await?;
                }
                Err(e) => {
                    tracing::warn!(movie_id = %movie_id, "Movie release dates skipped: {e:#}")
                }
            }
        }
        Some(api::RemoteSource::Tvdb) => anyhow::bail!("Unsupported movie sync source: TVDB"),
        _ => anyhow::bail!("Movie has no syncable remote"),
    }

    // Recompute the effective release date + pending entry from the movie's release filters before
    // broadcasting, so the emitted movie reflects the filtered release date.
    crate::background::update_movie_pending(db, movie_id).await?;

    let updated = db
        .movie_by_id(movie_id)
        .await?
        .context("Expected movie to exist after update")?;

    broadcast.broadcast_event(api::AppEventKind::MovieChanged { movie: updated });

    broadcast.broadcast_event(api::AppEventKind::TranslationsChanged {
        target: api::TranslationTarget::Movie(movie_id),
    });

    db.set_movie_synced_at(movie_id, api::Timestamp::now())
        .await?;
    broadcast.broadcast_event(api::AppEventKind::PendingChanged);
    tracing::info!(movie_id = %movie_id, "Sync complete");
    Ok(())
}

/// Fetch and replace a movie's per-language translated strings. Languages are
/// [`api::expand_sync_languages`] of the configured `sync_languages` against the
/// movie's own original language.
#[tracing::instrument(skip_all, fields(movie_id, tmdb_id))]
async fn collect_tmdb_movie_strings(
    movie_id: api::MovieId,
    tmdb_id: u32,
    info: &tmdb::MovieInfo,
    config: &api::Config,
    language: api::Locale,
    db: &Database,
    remote: &RemoteClients,
) -> Result<()> {
    let mut targets = api::expand_sync_languages(&config.sync_languages, info.original_language);

    // Always include the configured display locale so the shown title/overview
    // is stored even when it isn't one of the configured sync languages.
    let base = language.or(info.original_language);

    if !base.language().is_default() {
        targets.insert(base);
    }

    let translations = remote.fetch_tmdb_movie_translations(tmdb_id).await?;

    let mut rows: StringRows = Vec::new();

    for translation in translations {
        if !locale_matches_targets(translation.locale, &targets) {
            continue;
        }

        tracing::info!(?translation, ?targets, "TMDB movie translation row");

        push_string(
            &mut rows,
            translation.locale,
            api::StringKind::Title,
            translation.title.or(info.original_title.clone()),
        );

        push_string(
            &mut rows,
            translation.locale,
            api::StringKind::Overview,
            translation.overview,
        );
    }

    db.replace_movie_strings(movie_id, rows).await?;
    Ok(())
}
