use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use api::{
    EpisodeId, Image, ImageId, ImageKey, ImageKind, RemoteSource, SeasonNumber, SyncKind,
    SyncKindSet,
};

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::remote::RemoteClients;
use crate::shutdown::Shutdown;
use crate::tmdb;

/// Whether a sync layer fetched fresh data or detected (via ETag/`lastUpdated`)
/// that its remote is unchanged and skipped the expensive re-fetch.
enum LayerOutcome {
    Updated,
    Unchanged,
}

/// Serialize a remote's cache validators for the `*_remotes.cache` column,
/// yielding `None` (stored as `NULL`) when there's nothing worth caching so the
/// editor shows no stale "cached" state and the next sync sends no validator.
fn cache_json(cache: &api::RemoteCache) -> Option<String> {
    if cache.etag.is_none() && cache.last_updated.is_none() {
        return None;
    }

    serde_json::to_string(cache).ok()
}

/// The kinds a layer is about to fetch and persist this run, used both to tag a
/// fresh validator and to test an existing one for reuse.
fn needed_kinds(do_base: bool, do_air_date: bool) -> SyncKindSet {
    let mut kinds = SyncKindSet::empty();

    if do_base {
        kinds.insert(SyncKind::Base);
    }

    if do_air_date {
        kinds.insert(SyncKind::Dates);
    }

    kinds
}

/// Whether a layer may use this cached validator to short-circuit: skipping must be
/// rebuild-safe (`allow_skip`) and the cache must already cover every kind this run
/// needs - so an air-date-only validator never short-circuits a Base fetch after a
/// re-prioritization.
fn usable_cache(cache: &api::RemoteCache, needed: SyncKindSet, allow_skip: bool) -> bool {
    allow_skip && cache.kinds.contains_all(needed)
}

/// Flush the cache validators collected by Updated show layers. Called only after
/// the matching data has been persisted, so a stored marker always has its data.
async fn flush_show_cache_writes(
    writes: &[(api::RemoteId, Option<String>)],
    db: &Database,
) -> Result<()> {
    for (remote_id, cache) in writes {
        db.set_show_remote_cache(*remote_id, cache.clone()).await?;
    }

    Ok(())
}

pub(crate) async fn sync_show(
    show_id: api::ShowId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
    pending: &crate::pending::PendingSystem,
    shutdown: &Shutdown,
) -> Result<()> {
    let show = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist")?;

    let config = db.load_config().await?;

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

    tracing::info!(show_id = %show_id, title = show.strings.title(), "Syncing show");

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
        if shutdown.is_cancelled() {
            break;
        }

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
        let do_air_date = kinds.contains(SyncKind::Dates);

        // A short-circuit is only rebuild-safe when this layer is the Base
        // provider (skipping it routes to `persist_air_dates_only`, no rebuild) or
        // the Base provider already reported unchanged. Otherwise a fresh Base
        // elsewhere triggers a full rebuild that would wipe a skipped layer's data.
        let allow_skip = do_base || draft.base_unchanged;
        let cache = entry.cache.as_ref();

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
                        shutdown,
                        cache,
                        allow_skip,
                        entry.id,
                    )
                    .await
                }
                None => continue,
            },
            RemoteSource::Tvdb => match entry.remote.value().as_u32() {
                Some(tvdb_id) => {
                    tvdb_show_layer(
                        &mut draft,
                        &config,
                        tvdb_id,
                        do_base,
                        do_air_date,
                        remote,
                        shutdown,
                        cache,
                        allow_skip,
                        entry.id,
                    )
                    .await
                }
                None => continue,
            },
            RemoteSource::Tvmaze => match entry.remote.value().as_u32() {
                Some(tvmaze_id) => tvmaze_layer(&mut draft, show_id, tvmaze_id, remote)
                    .await
                    .map(|()| LayerOutcome::Updated),
                None => continue,
            },
            _ => continue,
        };

        // A failing layer shouldn't abort the sync: lower-priority layers and the
        // data already collected still persist, and the kind stays unclaimed so a
        // later layer can fill it.
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(e) => {
                tracing::warn!(?source, "Sync layer failed for show {show_id}: {e:#}");
                continue;
            }
        };

        match outcome {
            // Cache hit: claim the kinds so lower-priority layers skip the
            // exclusive Base kind (this source stays the owner), but keep the
            // existing data - don't record an air-date source (its stored
            // releases are preserved) and flag base so persist doesn't rebuild.
            LayerOutcome::Unchanged => {
                if kinds.contains(SyncKind::Base) {
                    draft.base_unchanged = true;
                }
            }
            LayerOutcome::Updated => {
                if do_air_date {
                    draft.air_date_sources.insert(source);
                }
            }
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

    // Don't persist a half-fetched draft: aborting here leaves the stored
    // strings/seasons untouched rather than truncating them via `replace_*`.
    if shutdown.is_cancelled() {
        anyhow::bail!("Sync aborted: service is shutting down");
    }

    // Base drives the show's seasons and episodes:
    //   - provided           → persist the fresh draft;
    //   - eligible, missing   → a configured Base source failed this run, so
    //                           keep the existing show rather than wiping it;
    //   - not eligible        → no enabled remote contributes Base, so the
    //                           seasons/episodes are orphaned and get cleared.
    if draft.provided.contains(SyncKind::Base) && !draft.base_unchanged {
        persist_show_draft(show_id, &show, &draft, db, broadcast).await?;
        flush_show_cache_writes(&draft.cache_writes, db).await?;
    } else if draft.base_unchanged {
        // The Base source was unchanged (cache hit): keep the stored
        // seasons/episodes/strings and only persist air dates other sources
        // produced this run.
        persist_air_dates_only(show_id, &draft, db).await?;
        flush_show_cache_writes(&draft.cache_writes, db).await?;
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
    score: f64,
}

/// A season's metadata contributed by the base layer.
#[derive(Default)]
struct SeasonDraft {
    tvdb_id: Option<u32>,
    air_date: Option<api::Timestamp>,
    poster: Option<Image>,
    /// This is set by tvdb to indicate that languages which are available for
    /// names.
    tvdb_translations: Arc<HashSet<String>>,
}

/// An episode's metadata contributed by the base layer.
struct EpisodeDraft {
    tvdb_id: Option<u32>,
    original_name: Option<String>,
    absolute_number: Option<u32>,
    aired: Option<api::Timestamp>,
    screenshot: Option<Image>,
    /// This is set by tvdb to indicate that languages which are available for
    /// names.
    tvdb_translations: Arc<HashSet<String>>,
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
    /// A Base-providing layer reported its remote unchanged (cache hit), so the
    /// existing seasons/episodes/strings are kept rather than rebuilt; only other
    /// sources' air dates are persisted. See the persist decision in `sync_show`.
    base_unchanged: bool,
    /// Cache validators (ETag/`lastUpdated`) captured by Updated layers, keyed by
    /// remote id, flushed to `*_remotes.cache` only after a successful persist so
    /// a failed persist never records a validator without its data.
    cache_writes: Vec<(api::RemoteId, Option<String>)>,
    original_name: Option<String>,
    first_air_date: Option<api::Timestamp>,
    /// The show's own original language, discovered from the Base layer.
    original_language: api::Locale,
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

    fn add_image(&mut self, kind: ImageKind, image: Image, score: f64, selected: bool) {
        if selected {
            self.selected
                .entry(kind)
                .or_insert_with(|| image.key().clone());
        }

        self.images.push(DraftImage { kind, image, score });
    }
}

#[tracing::instrument(skip_all, fields(tmdb_id, do_base, do_air_date))]
#[allow(clippy::too_many_arguments)]
async fn tmdb_show_layer(
    draft: &mut ShowDraft,
    config: &api::Config,
    show: &api::Show,
    tmdb_id: u32,
    do_base: bool,
    do_air_date: bool,
    remote: &RemoteClients,
    shutdown: &Shutdown,
    cache: Option<&api::RemoteCache>,
    allow_skip: bool,
    remote_id: api::RemoteId,
) -> Result<LayerOutcome> {
    tracing::info!(tmdb_id, do_base, do_air_date, "Show");

    let needed = needed_kinds(do_base, do_air_date);

    // Only replay the ETag when the cache already covers what we owe; a 304 has no
    // body, so we must force a full response when an uncovered kind is needed.
    let etag = cache
        .filter(|&c| usable_cache(c, needed, allow_skip))
        .and_then(|c| c.etag.as_deref());

    let info = match remote.fetch_tmdb_show(tmdb_id, etag).await? {
        tmdb::Conditional::NotModified => {
            tracing::info!(tmdb_id, "Show unchanged (ETag 304)");
            return Ok(LayerOutcome::Unchanged);
        }
        tmdb::Conditional::Modified { etag, value } => {
            if !needed.is_empty() {
                draft.cache_writes.push((
                    remote_id,
                    cache_json(&api::RemoteCache {
                        etag,
                        last_updated: None,
                        kinds: needed,
                    }),
                ));
            }
            value
        }
    };

    draft.original_name = info.original_name.or(draft.original_name.clone());

    for r in &info.remotes {
        draft.add_remote(r.slug.clone(), r.remote.clone());
    }

    // Graphics accumulate from every source.
    for (score, poster) in &info.posters {
        let selected = info.selected_poster.as_ref() == Some(poster.key());
        draft.add_image(ImageKind::Poster, poster.clone(), *score, selected);
    }

    for (score, backdrop) in &info.backdrops {
        let selected = info.selected_backdrop.as_ref() == Some(backdrop.key());
        draft.add_image(ImageKind::Backdrop, backdrop.clone(), *score, selected);
    }

    if !do_base && !do_air_date {
        return Ok(LayerOutcome::Updated);
    }

    if do_base {
        draft.first_air_date = info.first_air_date.or(show.first_air_date);
        draft.original_language = info.original_language;
        draft.base_remote = Some((RemoteSource::Tmdb, tmdb_id));
    }

    for season in &info.seasons {
        if shutdown.is_cancelled() {
            anyhow::bail!("Sync aborted: service is shutting down");
        }

        if do_base {
            let entry = draft.seasons.entry(season.number).or_default();
            entry.air_date = season.air_date;
            entry.poster = season.poster.clone().map(Image::from);
        }

        tracing::info!(tmdb_id, ?season.number, "Season episodes");

        for e in remote
            .fetch_tmdb_season_episodes(tmdb_id, season.number)
            .await?
        {
            if do_base {
                draft.episodes.insert(
                    (e.season, e.number),
                    EpisodeDraft {
                        tvdb_id: None,
                        original_name: e.original_name.clone(),
                        absolute_number: None,
                        aired: e.aired,
                        screenshot: e.filename.map(Image::from),
                        tvdb_translations: Arc::new(HashSet::new()),
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
        tracing::info!("Collecting strings");

        if let Err(error) =
            collect_tmdb_show_strings(draft, tmdb_id, show, config, remote, shutdown).await
        {
            tracing::warn!("String collection failed: {error:#}");
        }
    }

    Ok(LayerOutcome::Updated)
}

#[tracing::instrument(skip_all, fields(tvdb_id, do_base, do_air_date))]
#[allow(clippy::too_many_arguments)]
async fn tvdb_show_layer(
    draft: &mut ShowDraft,
    config: &api::Config,
    tvdb_id: u32,
    do_base: bool,
    do_air_date: bool,
    remote: &RemoteClients,
    shutdown: &Shutdown,
    cache: Option<&api::RemoteCache>,
    allow_skip: bool,
    remote_id: api::RemoteId,
) -> Result<LayerOutcome> {
    tracing::info!(tvdb_id, do_base, do_air_date, "Show");

    let needed = needed_kinds(do_base, do_air_date);

    let info = remote.fetch_tvdb_show(tvdb_id).await?;

    // TVDB has no ETag; the record-level `lastUpdated` marker detects an unchanged
    // series. An equal marker on a cache that already covers what we owe means the
    // whole entity (incl. episodes and air dates) is unchanged, so skip the episode
    // + translation fetches and keep the stored data.
    if let Some(cache) = cache
        && usable_cache(cache, needed, allow_skip)
        && let Some(cached) = cache.last_updated.as_deref()
        && let Some(current) = info.last_updated.as_deref()
        && cached == current
    {
        tracing::info!(tvdb_id, "Show unchanged (lastUpdated)");
        return Ok(LayerOutcome::Unchanged);
    }

    // Record the fresh marker so the next sync can compare (flushed post-persist).
    if !needed.is_empty() {
        draft.cache_writes.push((
            remote_id,
            cache_json(&api::RemoteCache {
                etag: None,
                last_updated: info.last_updated.clone(),
                kinds: needed,
            }),
        ));
    }

    for r in &info.remotes {
        draft.add_remote(r.slug.clone(), r.remote.clone());
    }

    for (score, poster) in &info.poster {
        let selected = info.selected_poster.as_ref() == Some(poster.key());
        draft.add_image(ImageKind::Poster, poster.clone(), *score, selected);
    }

    for (score, banner) in &info.banner {
        let selected = info.selected_banner.as_ref() == Some(banner.key());
        draft.add_image(ImageKind::Banner, banner.clone(), *score, selected);
    }

    for (score, fanart) in &info.fanart {
        let selected = info.selected_fanart.as_ref() == Some(fanart.key());
        draft.add_image(ImageKind::Backdrop, fanart.clone(), *score, selected);
    }

    if do_base {
        // TVDB has no first-air-date field; persist falls back to the existing value.
        draft.original_language = info.original_language;
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

            let mut translations = HashSet::new();

            translations.extend(s.name_translations.into_iter().map(|n| n.to_lowercase()));

            translations.extend(
                s.overview_translations
                    .into_iter()
                    .map(|n| n.to_lowercase()),
            );

            entry.tvdb_translations = Arc::new(translations);
        }
    }

    tracing::info!(tvdb_id, "Episodes");

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
                .collect::<HashSet<_>>();

            draft.episodes.insert(
                (e.season, e.number),
                EpisodeDraft {
                    tvdb_id: Some(e.id),
                    original_name: None,
                    absolute_number: e.absolute_number,
                    aired: e.aired,
                    screenshot,
                    tvdb_translations: Arc::new(translations),
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
        let targets = api::expand_sync_languages(&config.sync_languages, draft.original_language);

        // TVDB has no country dimension, so its strings are language-only.
        // Collapse each target to its language (country = DEFAULT) before
        // fetching; the `BTreeSet` then dedupes locales that differ only by
        // country.
        let tvdb_targets: BTreeSet<api::Locale> = targets
            .into_iter()
            .map(|l| api::Locale::new(l.language(), api::Country::DEFAULT))
            .collect();

        for language in tvdb_targets {
            if shutdown.is_cancelled() {
                anyhow::bail!("Sync aborted: service is shutting down");
            }

            tracing::info!(?language, "Collecting strings");

            // Remotes key on ISO 639-1; skip any locale whose language has no
            // 2-letter form.
            if language.language().to_part1().is_none() {
                continue;
            }

            if let Err(error) =
                collect_tvdb_strings(draft, tvdb_id, language, remote, shutdown).await
            {
                tracing::warn!("String collection failed: {error:#}");
            }
        }
    }

    Ok(LayerOutcome::Updated)
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

    tracing::info!(tvmaze_id, "Episodes");

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
fn locale_matches_targets(
    locale: api::Locale,
    targets: &BTreeSet<api::Locale>,
) -> Option<api::Locale> {
    for t in targets {
        if *t == locale || (t.country().is_default() && t.language() == locale.language()) {
            return Some(*t);
        }
    }

    None
}

#[tracing::instrument(skip_all, fields(tmdb_id))]
async fn collect_tmdb_show_strings(
    draft: &mut ShowDraft,
    tmdb_id: u32,
    show: &api::Show,
    config: &api::Config,
    remote: &RemoteClients,
    shutdown: &Shutdown,
) -> Result<()> {
    let language = show
        .language
        .or(config.language)
        .or(draft.original_language);

    let targets = api::expand_sync_languages(&config.sync_languages, language);

    let mut remaining = targets.clone();

    // Show strings: one translations call instead of one full-detail call per
    // language.
    let translations = remote.fetch_tmdb_show_translations(tmdb_id).await?;

    for translation in translations {
        let Some(locale) = locale_matches_targets(translation.locale, &targets) else {
            continue;
        };

        remaining.remove(&locale);

        tracing::info!(?translation, ?targets, "Show translation");

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

    for locale in remaining {
        draft.add_show_string(locale, api::StringKind::Title, draft.original_name.clone());
    }

    // Season strings: one translations call per season instead of one
    // full-detail call per season per language.
    let season_numbers: Vec<SeasonNumber> = draft.seasons.keys().copied().collect();

    for season_number in &season_numbers {
        if shutdown.is_cancelled() {
            anyhow::bail!("Sync aborted: service is shutting down");
        }

        let translations = remote
            .fetch_tmdb_season_translations(tmdb_id, *season_number)
            .await?;

        for translation in translations {
            if locale_matches_targets(translation.locale, &targets).is_none() {
                continue;
            }

            tracing::info!(?translation, ?targets, ?season_number, "Season translation");

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

    for (season, episode) in &episode_keys {
        if shutdown.is_cancelled() {
            anyhow::bail!("Sync aborted: service is shutting down");
        }

        let translations = remote
            .fetch_tmdb_episode_translations(tmdb_id, *season, *episode)
            .await?;

        let original_name = draft
            .episodes
            .get(&(*season, *episode))
            .and_then(|draft| draft.original_name.clone());

        for translation in translations {
            if locale_matches_targets(translation.locale, &targets).is_none() {
                continue;
            }

            tracing::info!(
                ?translation,
                ?targets,
                ?season,
                ?episode,
                "Episode translation"
            );

            draft.add_episode_string(
                *season,
                *episode,
                translation.locale,
                api::StringKind::Title,
                translation.name.or(original_name.clone()),
            );

            draft.add_episode_string(
                *season,
                *episode,
                translation.locale,
                api::StringKind::Overview,
                translation.overview,
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
    shutdown: &Shutdown,
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
            "Show translation"
        );

        draft.add_show_string(language, api::StringKind::Title, translation.name);
        draft.add_show_string(language, api::StringKind::Overview, translation.overview);
    }

    let seasons = draft
        .seasons
        .iter_mut()
        .flat_map(|(number, s)| Some((*number, s.tvdb_id?, s.tvdb_translations.clone())))
        .collect::<Vec<_>>();

    for (season, tvdb_id, translations) in seasons {
        if shutdown.is_cancelled() {
            anyhow::bail!("Sync aborted: service is shutting down");
        }

        if let Some(translation) = remote
            .fetch_tvdb_season_translation(tvdb_id, language, &translations)
            .await?
        {
            tracing::info!(
                ?tvdb_id,
                ?season,
                ?language,
                ?translation,
                "Season translation"
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
        .flat_map(|(key, e)| Some((*key, e.tvdb_id?, e.tvdb_translations.clone())))
        .collect::<Vec<_>>();

    for ((season, number), tvdb_id, translations) in episodes {
        if shutdown.is_cancelled() {
            anyhow::bail!("Sync aborted: service is shutting down");
        }

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
                "Episode translation"
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

    if !draft.original_language.is_default() {
        db.set_show_default_language(show_id, draft.original_language)
            .await?;
    }

    db.replace_show_strings(show_id, draft.show_strings.clone())
        .await?;

    for (slug, remote) in &draft.remotes {
        db.add_show_remote(show_id, slug.as_deref(), remote).await?;
    }

    // Graphics: replace all show images with the accumulated set, ranked in
    // source-priority (accumulation) order. Preserve the user's explicit pick
    // per kind if that image still exists; otherwise fall back to the
    // highest-priority default.
    let preserved = db.user_selected_show_image_keys(show_id).await?;

    db.clear_show_images(show_id).await?;

    let mut ranks: HashMap<ImageKind, u32> = HashMap::new();
    let mut user_ids: HashMap<ImageKind, ImageId> = HashMap::new();
    let mut default_ids: HashMap<ImageKind, ImageId> = HashMap::new();

    for draft_image in &draft.images {
        let id = ImageId::random();
        let rank = ranks.entry(draft_image.kind).or_default();

        db.upsert_show_image(
            id,
            show_id,
            draft_image.kind,
            *rank,
            &draft_image.image,
            Some(draft_image.score),
        )
        .await?;
        *rank += 1;

        if preserved.get(&draft_image.kind) == Some(draft_image.image.key()) {
            user_ids.entry(draft_image.kind).or_insert(id);
        }

        if draft.selected.get(&draft_image.kind) == Some(draft_image.image.key()) {
            default_ids.entry(draft_image.kind).or_insert(id);
        }
    }

    let kinds: HashSet<ImageKind> = user_ids.keys().chain(default_ids.keys()).copied().collect();

    for kind in kinds {
        if let Some(&id) = user_ids.get(&kind) {
            db.set_show_image_selection(show_id, kind, id, true).await?;
        } else if let Some(&id) = default_ids.get(&kind) {
            db.set_show_image_selection(show_id, kind, id, false)
                .await?;
        }
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

/// Persist only air-date releases when the Base layer was unchanged (cache hit):
/// the stored seasons/episodes/strings are kept, and we just upsert the releases
/// other sources produced this run (attributed to existing episodes) and prune
/// stale ones, scoped to the sources that actually ran. The caller's downstream
/// `recompute_episode_aired_for_show` + `EpisodesChanged` broadcasts surface any
/// resulting change.
async fn persist_air_dates_only(
    show_id: api::ShowId,
    draft: &ShowDraft,
    db: &Database,
) -> Result<()> {
    let episode_ids = db.episode_ids(show_id).await?;
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

    Ok(())
}

/// A movie release accumulated from a layer, attributed to its source so pruning
/// can be scoped like episode air dates.
struct MovieDraftRelease {
    source: RemoteSource,
    country: api::Country,
    release_type: api::ReleaseType,
    timestamp: api::Timestamp,
}

/// The movie analog of [`ShowDraft`]: the shared model the movie sync layers
/// contribute to before a single persist. Simpler than `ShowDraft` (no
/// seasons/episodes).
#[derive(Default)]
struct MovieDraft {
    provided: SyncKindSet,
    /// A Base layer reported its remote unchanged (cache hit): keep the stored
    /// metadata/images/strings and only persist other sources' fresh releases.
    base_unchanged: bool,
    /// Validators captured by Updated layers, flushed only after a successful
    /// persist (see [`flush_movie_cache_writes`]).
    cache_writes: Vec<(api::RemoteId, Option<String>)>,
    original_language: api::Locale,
    original_title: Option<String>,
    original_overview: Option<String>,
    remotes: Vec<(Option<String>, api::Remote)>,
    images: Vec<DraftImage>,
    selected: HashMap<ImageKind, ImageKey>,
    releases: Vec<MovieDraftRelease>,
    /// Sources whose release layer ran successfully this sync; scopes release
    /// pruning (mirrors [`ShowDraft::air_date_sources`]).
    release_sources: HashSet<RemoteSource>,
    strings: StringRows,
}

impl MovieDraft {
    fn needs(&self, kind: SyncKind) -> bool {
        !kind.is_exclusive() || !self.provided.contains(kind)
    }

    fn add_remote(&mut self, slug: Option<String>, remote: api::Remote) {
        self.remotes.push((slug, remote));
    }

    fn add_image(&mut self, kind: ImageKind, image: Image, score: f64, selected: bool) {
        if selected {
            self.selected
                .entry(kind)
                .or_insert_with(|| image.key().clone());
        }

        self.images.push(DraftImage { kind, image, score });
    }
}

pub(crate) async fn sync_movie(
    movie_id: api::MovieId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
    shutdown: &Shutdown,
) -> Result<()> {
    let movie = db
        .movie_by_id(movie_id)
        .await?
        .context("Expected movie to exist")?;

    let config = db.load_config().await?;

    tracing::info!(movie_id = %movie_id, title = movie.strings.title(), "Syncing movie");

    // Visit enabled remotes in priority order, one layer per source - mirroring
    // `sync_show`. For movies the AirDate kind carries release dates.
    let mut entries = movie
        .remotes
        .iter()
        .filter(|e| e.enabled)
        .collect::<Vec<_>>();

    entries.sort_by_key(|e| e.priority);

    let mut draft = MovieDraft::default();
    let mut seen = HashSet::new();

    for entry in entries {
        if shutdown.is_cancelled() {
            break;
        }

        let source = *entry.remote.source();

        if !seen.insert(source) {
            continue;
        }

        let configured = api::effective_remote_sync_kinds(entry, &config);
        let kinds: SyncKindSet = configured.iter().filter(|k| draft.needs(*k)).collect();

        if kinds.is_empty() && !source.has_graphics() {
            continue;
        }

        let do_base = kinds.contains(SyncKind::Base);
        let do_release = kinds.contains(SyncKind::Dates);

        let allow_skip = do_base || draft.base_unchanged;
        let cache = entry.cache.as_ref();

        let result = match source {
            RemoteSource::Tmdb => match entry.remote.value().as_u32() {
                Some(tmdb_id) => {
                    tmdb_movie_layer(
                        &mut draft, &config, &movie, tmdb_id, do_base, do_release, remote,
                        shutdown, cache, allow_skip, entry.id,
                    )
                    .await
                }
                None => continue,
            },
            // TVDB now has movies in its API, but no client layer is implemented
            // yet; surface it as a failure rather than silently skipping a source
            // the user enabled for sync.
            RemoteSource::Tvdb => Err(anyhow::anyhow!("TVDB movie sync is not yet supported")),
            // IMDb/Unknown are external-id references, not sync sources.
            _ => continue,
        };

        let outcome = match result {
            Ok(outcome) => outcome,
            Err(e) => {
                tracing::warn!(?source, "Sync layer failed for movie {movie_id}: {e:#}");
                continue;
            }
        };

        match outcome {
            LayerOutcome::Unchanged => {
                if kinds.contains(SyncKind::Base) {
                    draft.base_unchanged = true;
                }
            }
            LayerOutcome::Updated => {
                if do_release {
                    draft.release_sources.insert(source);
                }
            }
        }

        for k in kinds {
            draft.provided.insert(k);
        }
    }

    let eligible = api::eligible_sync_kinds(&movie.remotes, &config);

    if shutdown.is_cancelled() {
        anyhow::bail!("Sync aborted: service is shutting down");
    }

    if draft.provided.contains(SyncKind::Base) && !draft.base_unchanged {
        persist_movie_draft(movie_id, &draft, db).await?;
        flush_movie_cache_writes(&draft.cache_writes, db).await?;
    } else if draft.base_unchanged {
        // Base source unchanged (cache hit): keep stored metadata/strings/images,
        // persist only other sources' fresh releases.
        persist_movie_releases_only(movie_id, &draft, db).await?;
        flush_movie_cache_writes(&draft.cache_writes, db).await?;
    } else if eligible.contains(SyncKind::Base) {
        anyhow::bail!("Movie has no syncable Base remote available");
    } else {
        // No enabled remote contributes Base: the movie's derived metadata is
        // orphaned, so clear it (mirrors the show clear branch).
        db.replace_movie_strings(movie_id, Vec::new()).await?;
        db.prune_movie_releases(movie_id, &HashSet::new(), &HashSet::new())
            .await?;
    }

    // Recompute the effective release date + pending entry from the movie's release filters before
    // broadcasting, so the emitted movie reflects the filtered release date.
    db.update_movie_pending(movie_id, config.release_filters.clone())
        .await?;

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

/// Flush the cache validators collected by Updated movie layers, after persist.
async fn flush_movie_cache_writes(
    writes: &[(api::RemoteId, Option<String>)],
    db: &Database,
) -> Result<()> {
    for (remote_id, cache) in writes {
        db.set_movie_remote_cache(*remote_id, cache.clone()).await?;
    }

    Ok(())
}

/// The TMDB movie layer: fetch details conditionally (ETag), accumulating
/// metadata, images, remotes, strings and releases into the draft. Returns
/// [`LayerOutcome::Unchanged`] on a 304.
#[tracing::instrument(skip_all, fields(tmdb_id, do_base, do_release))]
#[allow(clippy::too_many_arguments)]
async fn tmdb_movie_layer(
    draft: &mut MovieDraft,
    config: &api::Config,
    movie: &api::Movie,
    tmdb_id: u32,
    do_base: bool,
    do_release: bool,
    remote: &RemoteClients,
    shutdown: &Shutdown,
    cache: Option<&api::RemoteCache>,
    allow_skip: bool,
    remote_id: api::RemoteId,
) -> Result<LayerOutcome> {
    tracing::info!(tmdb_id, do_base, do_release, "Movie");

    let needed = needed_kinds(do_base, do_release);

    let etag = cache
        .filter(|&c| usable_cache(c, needed, allow_skip))
        .and_then(|c| c.etag.as_deref());

    let info = match remote.fetch_tmdb_movie(tmdb_id, etag).await? {
        tmdb::Conditional::NotModified => {
            tracing::info!(tmdb_id, "Movie unchanged (ETag 304)");
            return Ok(LayerOutcome::Unchanged);
        }
        tmdb::Conditional::Modified { etag, value } => {
            if !needed.is_empty() {
                draft.cache_writes.push((
                    remote_id,
                    cache_json(&api::RemoteCache {
                        etag,
                        last_updated: None,
                        kinds: needed,
                    }),
                ));
            }
            value
        }
    };

    // Graphics accumulate from every source.
    for (score, poster) in &info.posters {
        let selected = info.selected_poster.as_ref() == Some(poster.key());
        draft.add_image(ImageKind::Poster, poster.clone(), *score, selected);
    }

    for (score, backdrop) in &info.backdrops {
        let selected = info.selected_backdrop.as_ref() == Some(backdrop.key());
        draft.add_image(ImageKind::Backdrop, backdrop.clone(), *score, selected);
    }

    for r in &info.remotes {
        draft.add_remote(None, r.clone());
    }

    if do_base {
        draft.original_language = info.original_language;
        draft.original_title = info.original_title.clone().or(draft.original_title.take());
        draft.original_overview = info
            .original_overview
            .clone()
            .or(draft.original_overview.take());

        match collect_tmdb_movie_strings(tmdb_id, &info, movie, config, remote, shutdown).await {
            Ok(rows) => draft.strings = rows,
            Err(error) => tracing::warn!("String collection failed: {error:#}"),
        }
    }

    if do_release {
        match remote.fetch_tmdb_movie_releases(tmdb_id).await {
            Ok(releases) => {
                tracing::info!(count = releases.len(), "Fetched TMDB movie releases");

                for r in releases {
                    draft.releases.push(MovieDraftRelease {
                        source: RemoteSource::Tmdb,
                        country: r.country,
                        release_type: r.release_type,
                        timestamp: r.release_date,
                    });
                }
            }
            Err(e) => tracing::warn!("Movie release dates skipped: {e:#}"),
        }
    }

    Ok(LayerOutcome::Updated)
}

/// Write the accumulated [`MovieDraft`] in one pass: base language, strings,
/// discovered remotes, accumulated graphics (ranked, with selection) and
/// per-source releases; prune releases no longer reported.
async fn persist_movie_draft(
    movie_id: api::MovieId,
    draft: &MovieDraft,
    db: &Database,
) -> Result<()> {
    if !draft.original_language.is_default() {
        db.set_movie_default_language(movie_id, draft.original_language)
            .await?;
    }

    db.replace_movie_strings(movie_id, draft.strings.clone())
        .await?;

    for (slug, remote) in &draft.remotes {
        db.add_movie_remote(movie_id, slug.as_deref(), remote)
            .await?;
    }

    // Graphics: replace all movie images with the accumulated set, ranked in
    // source-priority order. Preserve the user's explicit pick per kind if that
    // image still exists; otherwise fall back to the highest-priority default.
    let preserved = db.user_selected_movie_image_keys(movie_id).await?;

    db.clear_movie_images(movie_id).await?;

    let mut ranks: HashMap<ImageKind, u32> = HashMap::new();
    let mut user_ids: HashMap<ImageKind, ImageId> = HashMap::new();
    let mut default_ids: HashMap<ImageKind, ImageId> = HashMap::new();

    for draft_image in &draft.images {
        let id = ImageId::random();
        let rank = ranks.entry(draft_image.kind).or_default();

        db.upsert_movie_image(
            id,
            movie_id,
            draft_image.kind,
            *rank,
            &draft_image.image,
            Some(draft_image.score),
        )
        .await?;
        *rank += 1;

        if preserved.get(&draft_image.kind) == Some(draft_image.image.key()) {
            user_ids.entry(draft_image.kind).or_insert(id);
        }

        if draft.selected.get(&draft_image.kind) == Some(draft_image.image.key()) {
            default_ids.entry(draft_image.kind).or_insert(id);
        }
    }

    // Resolve each kind to (image, user-chosen?): a preserved user pick wins,
    // else the highest-priority default.
    let kinds: HashSet<ImageKind> = user_ids.keys().chain(default_ids.keys()).copied().collect();
    let mut resolved: HashMap<ImageKind, (ImageId, bool)> = HashMap::new();

    for kind in kinds {
        if let Some(&id) = user_ids.get(&kind) {
            resolved.insert(kind, (id, true));
        } else if let Some(&id) = default_ids.get(&kind) {
            resolved.insert(kind, (id, false));
        }
    }

    for (kind, (id, user_selected)) in &resolved {
        db.set_movie_image_selection(movie_id, *kind, *id, *user_selected)
            .await?;
    }

    // A movie has no banner artwork of its own; mirror the backdrop selection so
    // banner slots display the backdrop (as the previous inline sync did).
    if let Some(&(id, user_selected)) = resolved.get(&ImageKind::Backdrop) {
        db.set_movie_image_selection(movie_id, ImageKind::Banner, id, user_selected)
            .await?;
    }

    persist_movie_releases(movie_id, draft, db).await?;

    Ok(())
}

/// Persist only the releases from a cache-hit movie sync: the base layer reported
/// unchanged, so stored metadata/strings/images are kept and only releases other
/// sources produced this run are written.
async fn persist_movie_releases_only(
    movie_id: api::MovieId,
    draft: &MovieDraft,
    db: &Database,
) -> Result<()> {
    persist_movie_releases(movie_id, draft, db).await
}

/// Upsert the draft's releases and prune stale ones, scoped to the sources whose
/// release layer ran this sync. Shared by the full and cache-hit persist paths.
async fn persist_movie_releases(
    movie_id: api::MovieId,
    draft: &MovieDraft,
    db: &Database,
) -> Result<()> {
    let mut kept = HashSet::new();

    for r in &draft.releases {
        db.upsert_movie_release(movie_id, r.source, r.country, r.release_type, &r.timestamp)
            .await?;

        kept.insert((r.source, r.country, r.release_type));
    }

    db.prune_movie_releases(movie_id, &kept, &draft.release_sources)
        .await?;

    Ok(())
}

/// Fetch and replace a movie's per-language translated strings. Languages are
/// [`api::expand_sync_languages`] of the configured `sync_languages` against the
/// movie's own original language.
#[tracing::instrument(skip_all, fields(tmdb_id))]
async fn collect_tmdb_movie_strings(
    tmdb_id: u32,
    info: &tmdb::MovieInfo,
    movie: &api::Movie,
    config: &api::Config,
    remote: &RemoteClients,
    shutdown: &Shutdown,
) -> Result<StringRows> {
    let language = movie
        .language
        .or(config.language)
        .or(info.original_language);

    let targets = api::expand_sync_languages(&config.sync_languages, language);

    let translations = remote.fetch_tmdb_movie_translations(tmdb_id).await?;

    let mut remaining = targets.clone();
    let mut rows: StringRows = Vec::new();

    for translation in translations {
        if shutdown.is_cancelled() {
            anyhow::bail!("Sync aborted: service is shutting down");
        }

        let Some(locale) = locale_matches_targets(translation.locale, &targets) else {
            continue;
        };

        remaining.remove(&locale);

        tracing::info!(?translation, ?targets, ?language, "Movie translation");

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
            translation.overview.or(info.original_overview.clone()),
        );
    }

    if remaining.contains(&info.original_language) {
        push_string(
            &mut rows,
            info.original_language,
            api::StringKind::Title,
            info.original_title.clone(),
        );

        push_string(
            &mut rows,
            info.original_language,
            api::StringKind::Overview,
            info.original_overview.clone(),
        );
    }

    Ok(rows)
}
