use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use api::{
    EpisodeId, Image, ImageId, ImageKey, ImageKind, RemoteSource, SeasonNumber, SyncKind,
    SyncKindSet,
};

use crate::app_broadcast::Broadcaster;
use crate::db::{Database, InnerWrite};
use crate::remote::RemoteClients;
use crate::shutdown::Shutdown;
use crate::tmdb;

/// Whether a sync layer fetched fresh data or detected (via ETag/`lastUpdated`)
/// that its remote is unchanged and skipped the expensive re-fetch.
///
/// The validator a layer earns is not carried here - it is handed to [`CacheState`] via
/// [`CacheState::earn`] as the layer's last act, so that a layer which aborts partway
/// leaves no marker behind while its recorded errors still survive.
enum LayerOutcome {
    Updated,
    Unchanged,
    /// The remote does not carry this entity, or the fetch for it failed and was
    /// recorded. Either way the layer contributed nothing, so it claims no kinds -
    /// crucially not the exclusive `Base`, which a lower-priority source must still be
    /// free to provide.
    Absent,
}

/// Serialize a remote's cache validators for the `*_remotes.cache` column,
/// yielding `None` (stored as `NULL`) when there's nothing worth caching so the
/// editor shows no stale "cached" state and the next sync sends no validator.
fn cache_json(cache: &api::RemoteCache) -> Option<String> {
    if cache.etag.is_none() && cache.last_updated.is_none() && cache.errors.is_empty() {
        return None;
    }

    serde_json::to_string(cache).ok()
}

/// Stable [`api::RemoteError`] keys. A key must identify the *sub-request*, not the
/// attempt, so the next sync can recognise that this same call failed before.
fn season_episodes_key(season: SeasonNumber) -> String {
    format!("season/{}/episodes", season.ordinal())
}

fn episode_key(season: SeasonNumber, number: u32) -> String {
    format!("episode/{}", api::Code::new(season, number))
}

fn translations_key() -> &'static str {
    "translations"
}

fn credits_key() -> &'static str {
    "credits"
}

fn person_images_key() -> &'static str {
    "images"
}

fn episode_translations_key(season: SeasonNumber, number: u32) -> String {
    format!("episode/{}/translations", api::Code::new(season, number))
}

/// Classify a failed sub-request. A `404` means the remote genuinely does not carry
/// the entity, which is a stable fact worth trusting for a while; anything else is
/// treated as transient and re-probed soon.
fn classify_error(error: &anyhow::Error) -> api::RemoteErrorKind {
    for cause in error.chain() {
        if let Some(e) = cause.downcast_ref::<reqwest::Error>()
            && e.status() == Some(reqwest::StatusCode::NOT_FOUND)
        {
            return api::RemoteErrorKind::Missing;
        }
    }

    api::RemoteErrorKind::Transient
}

/// What a layer learned about one remote: the sub-requests that failed, and - only if
/// it completed everything it owed - the validator it earned.
///
/// The driver owns this and reads it back whether the layer returned `Ok` or `Err`, so
/// a failure is recorded (and therefore suppressed on the next run) even when the layer
/// aborts partway. The validator is the opposite: it is only ever set by [`Self::earn`]
/// as a layer's last act, so a marker can never outlive the data it stands for.
struct CacheState {
    /// The previous sync's cache, consulted to decide whether a sub-request that
    /// failed before is still suppressed.
    prior: Option<api::RemoteCache>,
    now: api::Timestamp,
    /// Failures this run: carried forward from `prior` while still live, or freshly
    /// recorded. Starts empty each run, so a sub-request that succeeds simply stops
    /// contributing an entry and its old error disappears.
    errors: Vec<api::RemoteError>,
    earned: Option<api::RemoteCache>,
}

impl CacheState {
    fn new(prior: Option<&api::RemoteCache>, now: api::Timestamp) -> Self {
        Self {
            prior: prior.cloned(),
            now,
            errors: Vec::new(),
            earned: None,
        }
    }

    /// Whether `key` failed recently enough to skip entirely. The carried-forward
    /// error keeps its original timestamp, so suppression expires when it was always
    /// going to rather than being renewed by each skip.
    fn suppressed(&mut self, key: &str) -> bool {
        let Some(error) = self.prior.as_ref().and_then(|c| c.error(key)) else {
            return false;
        };

        if !error.is_live(self.now) {
            return false;
        }

        let error = error.clone();
        tracing::debug!(key, "Skipping sub-request: recent failure still cached");
        self.errors.push(error);
        true
    }

    fn record(&mut self, key: &str, error: &anyhow::Error) {
        let kind = classify_error(error);
        tracing::warn!(key, ?kind, "Sub-request failed: {error:#}");

        self.errors.push(api::RemoteError {
            key: key.to_owned(),
            message: format!("{error:#}"),
            kind,
            at: self.now,
        });
    }

    /// Record a failure the client reported as a plain absence rather than an error
    /// (an empty result where an entity was expected).
    fn record_missing(&mut self, key: &str, message: impl Into<String>) {
        let message = message.into();
        tracing::info!(key, "{message}");

        self.errors.push(api::RemoteError {
            key: key.to_owned(),
            message,
            kind: api::RemoteErrorKind::Missing,
            at: self.now,
        });
    }

    /// Whether anything failed (or is still suppressed) this run. A layer in this state
    /// produced an incomplete draft, so the persist must not treat absence as removal.
    fn degraded(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Called by a layer only after every fetch it owed has succeeded.
    fn earn(&mut self, cache: api::RemoteCache) {
        self.earned = Some(cache);
    }

    /// The row to store, carrying every error seen this run.
    ///
    /// The validator it keeps depends on `persisted` - whether the data this layer
    /// fetched actually landed in the database:
    ///
    /// - persisted: the freshly earned validator, or the prior one when the layer
    ///   short-circuited (a `304` earns nothing, but the validator that *produced* the
    ///   `304` is still valid and must not be dropped);
    /// - not persisted: only the prior validator. A newly earned one would claim data
    ///   that was never stored, and the next sync's `304` would then report the source
    ///   unchanged while its data is absent.
    ///
    /// Either way the prior validator survives a failed layer, so a transient error
    /// doesn't cost a full re-fetch on the next run.
    fn finish(&self, persisted: bool) -> api::RemoteCache {
        let validator = if persisted {
            self.earned.clone().or_else(|| self.prior.clone())
        } else {
            self.prior.clone()
        };

        let mut cache = validator.unwrap_or(api::RemoteCache {
            etag: None,
            last_updated: None,
            kinds: SyncKindSet::empty(),
            errors: Vec::new(),
        });

        cache.errors = self.errors.clone();
        cache
    }
}

/// Run a sub-request, recovering from failure instead of aborting the layer: the error
/// is recorded against `key` and the caller gets `None`, so the rest of the layer still
/// contributes what it can.
///
/// A `key` that failed recently is not retried at all. That is the point: a remote which
/// simply does not carry an entity (a `404`) would otherwise cost one wasted call on
/// every single sync, forever.
async fn recover<T>(
    state: &mut CacheState,
    key: &str,
    f: impl Future<Output = Result<T>>,
) -> Option<T> {
    if state.suppressed(key) {
        return None;
    }

    match f.await {
        Ok(value) => Some(value),
        Err(error) => {
            state.record(key, &error);
            None
        }
    }
}

/// The kinds a layer is about to fetch and persist this run, used both to tag a
/// fresh validator and to test an existing one for reuse.
/// The image-table source enum for a remote source. Only graphics-capable
/// sources (TMDB/TVDB) are ever passed here.
fn image_source(source: RemoteSource) -> api::ImageSource {
    match source {
        RemoteSource::Tvdb => api::ImageSource::Tvdb,
        RemoteSource::Tmdb => api::ImageSource::Tmdb,
        _ => api::ImageSource::Unknown,
    }
}

fn needed_kinds(do_base: bool, do_air_date: bool, do_credits: bool) -> SyncKindSet {
    let mut kinds = SyncKindSet::empty();

    if do_base {
        kinds.insert(SyncKind::Base);
    }

    if do_air_date {
        kinds.insert(SyncKind::Dates);
    }

    // Credits come from a separate endpoint but share the base ETag; including the
    // kind here forces a full fetch (rather than a 304 that skips the credits
    // sub-request) whenever credits are owed but the cache doesn't yet cover them.
    if do_credits {
        kinds.insert(SyncKind::Credits);
    }

    kinds
}

/// Whether a layer may use this cached validator to short-circuit: skipping must be
/// rebuild-safe (`allow_skip`), the cache must already cover every kind this run
/// needs - so an air-date-only validator never short-circuits a Base fetch after a
/// re-prioritization - and no recorded failure may be due for a retry.
///
/// That last clause is what lets a failed sub-request ever run again: short-circuiting
/// returns before any sub-request is reached, so a cache holding an expired error must
/// force the full fetch. While its errors are still live we *do* short-circuit, which is
/// exactly the API call we are trying to save.
fn usable_cache(
    cache: &api::RemoteCache,
    needed: SyncKindSet,
    allow_skip: bool,
    now: api::Timestamp,
) -> bool {
    allow_skip && cache.kinds.contains_all(needed) && !cache.has_expired_errors(now)
}

/// Flush what the show layers learned. `persisted` says whether the data they fetched
/// actually landed; see [`CacheState::finish`] for what that changes.
async fn flush_show_cache_writes(
    show_id: api::ShowId,
    writes: &[(api::RemoteId, CacheState)],
    db: &Database,
    persisted: bool,
) -> Result<()> {
    for (remote_id, state) in writes {
        db.set_remote_cache(show_id, *remote_id, cache_json(&state.finish(persisted)))
            .await?;
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
    let (show, config) = load_show_for_sync(show_id, db, remote).await?;

    tracing::info!(show_id = %show_id, title = show.strings.title(), "Syncing show");

    let draft = run_show_layers(&show, &config, remote, shutdown).await;

    // Don't persist a half-fetched draft: aborting here leaves the stored
    // strings/seasons untouched rather than truncating them via `replace_*`.
    if shutdown.is_cancelled() {
        anyhow::bail!("Sync aborted: service is shutting down");
    }

    persist_show_sync(&show, &config, draft, db, broadcast).await?;
    finish_show_sync(show_id, &config, db, broadcast, pending).await
}

/// Load the show and the sync config for its viewers, first storing a TVmaze
/// remote when the show has none yet.
async fn load_show_for_sync(
    show_id: api::ShowId,
    db: &Database,
    remote: &RemoteClients,
) -> Result<(api::Show, api::Config)> {
    let show = db
        .show_by_id(None, show_id)
        .await?
        .context("Expected show to exist")?;

    let mut config = db.load_config().await?;
    config.sync_languages = api::sync_languages_for_viewers(
        &config.sync_languages,
        &db.show_viewer_languages(show_id).await?,
    );

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
        .show_by_id(None, show_id)
        .await?
        .context("Expected show to exist")?;

    Ok((show, config))
}

/// Visit enabled remotes in priority order, one layer per source (the
/// highest-priority entry of each source wins), each contributing the kinds it's
/// configured for (global default, or its own override) to a shared draft.
async fn run_show_layers(
    show: &api::Show,
    config: &api::Config,
    remote: &RemoteClients,
    shutdown: &Shutdown,
) -> ShowDraft {
    // One clock for the whole sync, so every error recorded this run shares a timestamp
    // and expires together.
    let now = api::Timestamp::now();

    let mut entries = show
        .remotes
        .iter()
        .filter(|e| e.enabled)
        .collect::<Vec<_>>();

    entries.sort_by_key(|e| e.priority);

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
        let configured = api::effective_remote_sync_kinds(entry, config);
        let kinds: SyncKindSet = configured.iter().filter(|k| draft.needs(*k)).collect();

        // Run the source if it still owes a kind, or just to accumulate graphics.
        if kinds.is_empty() && !source.has_graphics() {
            continue;
        }

        let do_base = kinds.contains(SyncKind::Base);

        let cx = ShowLayer {
            config,
            remote,
            shutdown,
            do_base,
            do_air_date: kinds.contains(SyncKind::Dates),
            do_credits: kinds.contains(SyncKind::Credits),
            // A short-circuit is only rebuild-safe when this layer is the Base
            // provider (skipping it routes to `persist_air_dates_only`, no rebuild) or
            // the Base provider already reported unchanged. Otherwise a fresh Base
            // elsewhere triggers a full rebuild that would wipe a skipped layer's data.
            allow_skip: do_base || draft.base_unchanged,
        };

        let mut state = CacheState::new(entry.cache.as_ref(), now);

        let Some(result) = run_show_layer(cx, &mut draft, &mut state, show, entry).await else {
            continue;
        };

        // Whatever the layer learned about this remote is recorded either way: a
        // validator only if it earned one, but its errors even when it aborted - that
        // is what stops a dead sub-request being re-probed on every single sync.
        let degraded = state.degraded();

        if degraded {
            draft.degraded_sources.insert(source);
        }

        draft.cache_writes.push((entry.id, state));

        // A failing layer shouldn't abort the sync: lower-priority layers and the
        // data already collected still persist, and the kind stays unclaimed so a
        // later layer can fill it.
        match result {
            Ok(outcome) => draft.claim(source, kinds, outcome, degraded),
            Err(e) => {
                tracing::warn!(?source, "Sync layer failed for show {}: {e:#}", show.id);
            }
        }
    }

    draft
}

/// Run the layer for one remote entry, or `None` when its source has no show
/// layer or its id is unusable.
async fn run_show_layer(
    cx: ShowLayer<'_>,
    draft: &mut ShowDraft,
    state: &mut CacheState,
    show: &api::Show,
    entry: &api::RemoteEntry,
) -> Option<Result<LayerOutcome>> {
    let id = entry.remote.value().as_u32()?;

    let result = match *entry.remote.source() {
        RemoteSource::Tmdb => tmdb_show_layer(cx, draft, state, show, id).await,
        RemoteSource::Tvdb => tvdb_show_layer(cx, draft, state, id).await,
        // TVmaze offers no conditional request, so it never earns a validator.
        RemoteSource::Tvmaze => tvmaze_layer(draft, state, show.id, id, cx.remote)
            .await
            .map(|()| LayerOutcome::Updated),
        _ => return None,
    };

    Some(result)
}

/// Persist the draft according to how its Base kind was (or wasn't) provided,
/// flushing every layer's cache state alongside.
async fn persist_show_sync(
    show: &api::Show,
    config: &api::Config,
    draft: ShowDraft,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    let show_id = show.id;

    // The kinds at least one enabled remote is configured to contribute. This
    // tells a deliberately-excluded kind (clear its derived data) apart from a
    // transient fetch failure (keep what's already stored), mirroring how air
    // dates use eligibility in `recompute_episode_aired_for_show`.
    let eligible = api::eligible_sync_kinds(&show.remotes, config);

    // Base drives the show's seasons and episodes:
    //   - provided           → persist the fresh draft;
    //   - eligible, missing   → a configured Base source failed this run, so
    //                           keep the existing show rather than wiping it;
    //   - not eligible        → no enabled remote contributes Base; keep the
    //                           stored seasons/episodes too, since deleting them
    //                           would cascade to every user's pending rows.
    let draft = Arc::new(draft);

    if draft.provided.contains(SyncKind::Base) && !draft.base_unchanged {
        persist_show_draft(show_id, show, &draft, db, broadcast).await?;
        flush_show_cache_writes(show_id, &draft.cache_writes, db, true).await?;
    } else if draft.base_unchanged {
        // The Base source was unchanged (cache hit): keep the stored
        // seasons/episodes/strings and only persist air dates other sources
        // produced this run.
        let air_dates = Arc::clone(&draft);
        db.transaction(move |s| persist_air_dates_only(show_id, &air_dates, s))
            .await?;
        flush_show_cache_writes(show_id, &draft.cache_writes, db, true).await?;

        // A non-base source may have merged fresh graphics (or a slug); push the
        // refreshed show so clients update without a manual reload.
        if !draft.graphics_sources.is_empty()
            && let Some(show) = db.show_by_id(None, show_id).await?
        {
            broadcast.broadcast_event(api::AppEventKind::ShowChanged { show });
        }
    } else if eligible.contains(SyncKind::Base) {
        // Nothing persisted, so no validator may be stored - but the failures that got
        // us here must be, or we'd re-probe a dead remote on every sync.
        flush_show_cache_writes(show_id, &draft.cache_writes, db, false).await?;

        // A Base source that reported *why* it produced nothing (the remote 404s, its
        // episode list is unreachable) is an ordinary fact about the remote, already
        // recorded and cached. Only an unexplained absence is a fault worth failing on.
        if draft.degraded_sources.is_empty() {
            anyhow::bail!("Show has no syncable Base remote available");
        }

        tracing::warn!(
            sources = ?draft.degraded_sources,
            "No Base source produced data; keeping the stored show"
        );
    } else {
        flush_show_cache_writes(show_id, &draft.cache_writes, db, false).await?;

        tracing::warn!(show_id = %show_id, "No enabled remote provides Base; keeping the stored seasons");
    }

    Ok(())
}

/// Recompute effective air dates and pending rows after a sync, and tell clients.
async fn finish_show_sync(
    show_id: api::ShowId,
    config: &api::Config,
    db: &Database,
    broadcast: &Broadcaster,
    pending: &crate::pending::PendingSystem,
) -> Result<()> {
    // Merge all sources' air dates into the effective episodes.aired by priority,
    // then broadcast each season so clients pick up the recomputed dates.
    db.recompute_episode_aired_for_show(show_id, config.air_date_filters.clone())
        .await?;

    for season in db.seasons(None, show_id).await? {
        broadcast.broadcast_event(api::AppEventKind::EpisodesChanged {
            show_id,
            season: season.season,
        });
    }

    let now = api::Timestamp::now();
    pending.fill_for_show(show_id, now).await?;
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
    db.add_remote(show_id, None, &api::Remote::tvmaze(tvmaze_id))
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

/// A cast/crew credit accumulated during sync: the person's identity and role,
/// plus the person's name and the character name in each synced language. Merged
/// across the per-language credit fetches by the remote's stable credit id. The
/// person's name is only a placeholder seed here - the authoritative localized
/// name comes from the independent person sync.
struct CreditDraft {
    source: RemoteSource,
    remote_person_id: u32,
    profile: Option<Image>,
    kind: api::CreditKind,
    department: Option<String>,
    job: Option<String>,
    order: Option<u32>,
    episode_count: Option<u32>,
    /// `(locale, name)` seed for the person, one per synced language.
    names: Vec<(api::Locale, String)>,
    /// `(locale, character)`, one entry per synced language that returned a
    /// character name.
    characters: Vec<(api::Locale, String)>,
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
    /// existing seasons/episodes/strings are kept rather than rebuilt; only
    /// other sources' air dates are persisted. See the persist decision in
    /// `sync_show`.
    base_unchanged: bool,
    /// Cache validators (ETag/`lastUpdated`) captured by Updated layers, keyed
    /// by remote id, flushed to `*_remotes.cache` only after a successful
    /// persist so a failed persist never records a validator without its data.
    cache_writes: Vec<(api::RemoteId, CacheState)>,
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
    /// Sources that recovered from at least one failed sub-request this run, so their
    /// contribution to the draft is incomplete. Absence from an incomplete draft is not
    /// evidence of removal, so the persist must not prune against it.
    degraded_sources: HashSet<RemoteSource>,
    images: Vec<DraftImage>,
    /// Sources whose graphics layer ran (returned Updated) this sync, so their
    /// `images` in the draft are authoritative. Scopes the per-source graphics
    /// merge in the base-unchanged path (mirrors [`air_date_sources`]).
    graphics_sources: HashSet<RemoteSource>,
    selected: HashMap<ImageKind, ImageKey>,
    /// Per-language translated strings keyed by owner, populated for every target
    /// remote.
    show_strings: StringRows,
    season_strings: BTreeMap<SeasonNumber, StringRows>,
    episode_strings: BTreeMap<(SeasonNumber, u32), StringRows>,
    /// This is set by tvdb to indicate that languages which are available for
    /// names.
    translations: HashSet<String>,
    /// Cast & crew, provided by the TMDB Credits layer.
    credits: Vec<CreditDraft>,
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

    /// Record what a layer that ran contributed, claiming the kinds it owed
    /// unless it was absent.
    fn claim(
        &mut self,
        source: RemoteSource,
        kinds: SyncKindSet,
        outcome: LayerOutcome,
        degraded: bool,
    ) {
        match outcome {
            // Contributed nothing, so claims nothing - a lower-priority source may
            // still provide the kinds this one owed.
            LayerOutcome::Absent => return,
            // Cache hit: claim the kinds so lower-priority layers skip the
            // exclusive Base kind (this source stays the owner), but keep the
            // existing data - don't record an air-date source (its stored
            // releases are preserved) and flag base so persist doesn't rebuild.
            LayerOutcome::Unchanged => {
                if kinds.contains(SyncKind::Base) {
                    self.base_unchanged = true;
                }
            }
            LayerOutcome::Updated => {
                // A degraded layer didn't report everything it has, so its stored air
                // dates must not be pruned against this run's partial view.
                if kinds.contains(SyncKind::Dates) && !degraded {
                    self.air_date_sources.insert(source);
                }

                // A source that ran fully re-supplied its graphics into the
                // draft; mark it so the base-unchanged path can refresh just
                // this source's stored images.
                if source.has_graphics() {
                    self.graphics_sources.insert(source);
                }
            }
        }

        for k in kinds {
            self.provided.insert(k);
        }
    }
}

/// What a show layer owes this run, and the shared services it fetches with.
#[derive(Clone, Copy)]
struct ShowLayer<'a> {
    config: &'a api::Config,
    remote: &'a RemoteClients,
    shutdown: &'a Shutdown,
    do_base: bool,
    do_air_date: bool,
    do_credits: bool,
    /// Whether a cached validator may short-circuit the layer; see `run_show_layers`.
    allow_skip: bool,
}

#[tracing::instrument(skip_all, fields(tmdb_id, do_base, do_air_date))]
async fn tmdb_show_layer(
    cx: ShowLayer<'_>,
    draft: &mut ShowDraft,
    state: &mut CacheState,
    show: &api::Show,
    tmdb_id: u32,
) -> Result<LayerOutcome> {
    let ShowLayer {
        config,
        remote,
        shutdown,
        do_base,
        do_air_date,
        do_credits,
        allow_skip,
    } = cx;

    tracing::info!(tmdb_id, do_base, do_air_date, do_credits, "Show");

    let needed = needed_kinds(do_base, do_air_date, do_credits);

    // Only replay the ETag when the cache already covers what we owe; a 304 has no
    // body, so we must force a full response when an uncovered kind is needed.
    let etag = state
        .prior
        .as_ref()
        .filter(|c| usable_cache(c, needed, allow_skip, state.now))
        .and_then(|c| c.etag.clone());

    let (fresh_etag, info) = match remote.fetch_tmdb_show(tmdb_id, etag.as_deref()).await? {
        tmdb::Conditional::NotModified => {
            tracing::info!(tmdb_id, "Show unchanged (ETag 304)");
            return Ok(LayerOutcome::Unchanged);
        }
        tmdb::Conditional::Modified { etag, value } => (etag, value),
    };

    // Built here, handed to `state` only on the success path below.
    let earned = (!needed.is_empty()).then_some(api::RemoteCache {
        etag: fresh_etag,
        last_updated: None,
        kinds: needed,
        errors: Vec::new(),
    });

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

    // Graphics-only run: nothing was owed, so nothing to earn.
    if !do_base && !do_air_date && !do_credits {
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

        // A season whose episodes we can't fetch must not sink the whole show: record
        // it, keep the other seasons, and let the persist skip pruning.
        let key = season_episodes_key(season.number);

        let Some(episodes) = recover(
            state,
            &key,
            remote.fetch_tmdb_season_episodes(tmdb_id, season.number),
        )
        .await
        else {
            continue;
        };

        for e in episodes {
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

        recover(
            state,
            translations_key(),
            collect_tmdb_show_strings(draft, tmdb_id, config, remote, shutdown),
        )
        .await;
    }

    if do_credits {
        tracing::info!("Collecting credits");

        recover(
            state,
            credits_key(),
            collect_tmdb_show_credits(draft, tmdb_id, config, remote, shutdown),
        )
        .await;
    }

    // Earned: every fetch this layer owed either succeeded or was recovered into
    // `state.errors`, which travel with the validator so the failed calls are retried
    // once they expire.
    if let Some(earned) = earned {
        state.earn(earned);
    }

    Ok(LayerOutcome::Updated)
}

#[tracing::instrument(skip_all, fields(tvdb_id, do_base, do_air_date))]
async fn tvdb_show_layer(
    cx: ShowLayer<'_>,
    draft: &mut ShowDraft,
    state: &mut CacheState,
    tvdb_id: u32,
) -> Result<LayerOutcome> {
    let ShowLayer {
        config,
        remote,
        shutdown,
        do_base,
        do_air_date,
        allow_skip,
        ..
    } = cx;

    tracing::info!(tvdb_id, do_base, do_air_date, "Show");

    let needed = needed_kinds(do_base, do_air_date, false);

    let info = remote.fetch_tvdb_show(tvdb_id).await?;

    // Record discovered remotes (notably the TVDB slug that external links need)
    // before any unchanged short-circuit, so the slug is kept current even when
    // TVDB runs only to accumulate graphics under a higher-priority Base source.
    for r in &info.remotes {
        draft.add_remote(r.slug.clone(), r.remote.clone());
    }

    // TVDB has no ETag; the record-level `lastUpdated` marker detects an unchanged
    // series. An equal marker on a cache that already covers what we owe means the
    // whole entity (incl. episodes and air dates) is unchanged, so skip the episode
    // + translation fetches and keep the stored data.
    if let Some(cache) = state.prior.as_ref()
        && usable_cache(cache, needed, allow_skip, state.now)
        && let Some(cached) = cache.last_updated.as_deref()
        && let Some(current) = info.last_updated.as_deref()
        && cached == current
    {
        tracing::info!(tvdb_id, "Show unchanged (lastUpdated)");
        return Ok(LayerOutcome::Unchanged);
    }

    // The fresh marker the next sync compares against. Built here, handed to `state`
    // only on the success path below.
    let earned = (!needed.is_empty()).then(|| api::RemoteCache {
        etag: None,
        last_updated: info.last_updated.clone(),
        kinds: needed,
        errors: Vec::new(),
    });

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

    // TVDB serves the whole series' episodes in one paginated call, so a failure here
    // costs every episode. Recovering keeps the show's graphics/strings and leaves the
    // stored episodes alone (the persist won't prune against an incomplete draft).
    let episodes = recover(state, "episodes", remote.fetch_tvdb_episodes(tvdb_id))
        .await
        .unwrap_or_default();

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

            let key = format!("translations/{language}");

            recover(
                state,
                &key,
                collect_tvdb_strings(draft, tvdb_id, language, remote, shutdown),
            )
            .await;
        }
    }

    if let Some(earned) = earned {
        state.earn(earned);
    }

    Ok(LayerOutcome::Updated)
}

#[tracing::instrument(skip_all, fields(show_id))]
async fn tvmaze_layer(
    draft: &mut ShowDraft,
    state: &mut CacheState,
    show_id: api::ShowId,
    tvmaze_id: u32,
    remote: &RemoteClients,
) -> Result<()> {
    tracing::info!(show_id = %show_id, tvmaze_id, "TVmaze");

    let network = recover(
        state,
        "network",
        remote.fetch_tvmaze_show_network(tvmaze_id),
    )
    .await
    .unwrap_or_default();

    tracing::info!(tvmaze_id, "Episodes");

    let episodes = recover(state, "episodes", remote.fetch_tvmaze_episodes(tvmaze_id))
        .await
        .unwrap_or_default();

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

/// `original` as the text for `locale` when the remote has none, which only the
/// original language may use: TMDB leaves the original language's own entry
/// blank, while any other language without text is missing rather than the
/// original-language text.
fn original_fallback(
    locale: api::Locale,
    original_language: api::Locale,
    original: &Option<String>,
) -> Option<String> {
    if locale.language() == original_language.language() {
        original.clone()
    } else {
        None
    }
}

fn add_tmdb_show_strings(
    draft: &mut ShowDraft,
    translations: Vec<tmdb::Translation>,
    targets: &BTreeSet<api::Locale>,
) {
    let mut remaining = targets.clone();

    for translation in translations {
        let Some(locale) = locale_matches_targets(translation.locale, targets) else {
            continue;
        };

        remaining.remove(&locale);

        tracing::info!(?translation, ?targets, "Show translation");

        let title = translation.name.or_else(|| {
            original_fallback(
                translation.locale,
                draft.original_language,
                &draft.original_name,
            )
        });

        draft.add_show_string(translation.locale, api::StringKind::Title, title);

        draft.add_show_string(
            translation.locale,
            api::StringKind::Overview,
            translation.overview,
        );
    }

    for locale in remaining {
        let title = original_fallback(locale, draft.original_language, &draft.original_name);
        draft.add_show_string(locale, api::StringKind::Title, title);
    }
}

#[tracing::instrument(skip_all, fields(tmdb_id))]
async fn collect_tmdb_show_strings(
    draft: &mut ShowDraft,
    tmdb_id: u32,
    config: &api::Config,
    remote: &RemoteClients,
    shutdown: &Shutdown,
) -> Result<()> {
    let targets = api::expand_sync_languages(&config.sync_languages, draft.original_language);

    // Show strings: one translations call instead of one full-detail call per
    // language.
    let translations = remote.fetch_tmdb_show_translations(tmdb_id).await?;
    add_tmdb_show_strings(draft, translations, &targets);

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
                translation.name.or_else(|| {
                    original_fallback(translation.locale, draft.original_language, &original_name)
                }),
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
/// Merge one language's fetched credits into `merged`, keyed by the remote's
/// stable credit id. Person and role fields are set from the first language that
/// returns each credit; the character name is appended per language.
fn merge_credits(
    merged: &mut BTreeMap<String, CreditDraft>,
    credits: Vec<tmdb::CreditInfo>,
    locale: api::Locale,
) {
    for c in credits {
        let tmdb::CreditInfo {
            tmdb_credit_id,
            tmdb_person_id,
            name,
            profile_path,
            kind,
            character,
            department,
            job,
            order,
            episode_count,
        } = c;

        // No stable key means we can't dedupe this credit across languages.
        if tmdb_credit_id.is_empty() {
            continue;
        }

        let entry = merged.entry(tmdb_credit_id).or_insert_with(|| CreditDraft {
            source: RemoteSource::Tmdb,
            remote_person_id: tmdb_person_id,
            profile: profile_path.as_deref().map(Image::tmdb),
            kind,
            department,
            job,
            order,
            episode_count,
            names: Vec::new(),
            characters: Vec::new(),
        });

        if !name.trim().is_empty() {
            entry.names.push((locale, name));
        }

        if let Some(character) = character
            && !character.trim().is_empty()
        {
            entry.characters.push((locale, character));
        }
    }
}

/// Fetch cast & crew for a show once per synced language (TMDB's only way to get
/// translated character names) and accumulate them into the draft.
#[tracing::instrument(skip_all, fields(tmdb_id))]
async fn collect_tmdb_show_credits(
    draft: &mut ShowDraft,
    tmdb_id: u32,
    config: &api::Config,
    remote: &RemoteClients,
    shutdown: &Shutdown,
) -> Result<()> {
    let targets = api::expand_sync_languages(&config.sync_languages, draft.original_language);

    let mut merged: BTreeMap<String, CreditDraft> = BTreeMap::new();

    for locale in &targets {
        if shutdown.is_cancelled() {
            anyhow::bail!("Sync aborted: service is shutting down");
        }

        let credits = remote
            .fetch_tmdb_show_credits(tmdb_id, &locale.to_string())
            .await?;

        merge_credits(&mut merged, credits, *locale);
    }

    draft.credits = merged.into_values().collect();
    Ok(())
}

/// Turn a name into a URL slug: lowercase ASCII-alphanumerics, other runs collapsed
/// to single hyphens, no leading/trailing hyphen (e.g. `"Brad Pitt"` -> `"brad-pitt"`).
fn slugify(name: &str) -> String {
    let mut slug = String::new();

    for c in name.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }

    slug.trim_end_matches('-').to_owned()
}

/// The data a person layer accumulates before it is persisted in one pass.
#[derive(Default)]
struct PersonDraft {
    department: Option<String>,
    /// Remotes discovered from the source (e.g. an IMDb cross-reference), added to
    /// `person_remotes` like shows/movies rather than kept as a scalar column.
    remotes: Vec<(Option<String>, api::Remote)>,
    /// The person's canonical (primary) language, used as the display fallback.
    default_language: api::Locale,
    strings: StringRows,
    images: Vec<(f64, Image)>,
    /// A layer fetched fresh data (200); its draft should be persisted.
    updated: bool,
    /// A layer short-circuited on a 304; the stored data is still current.
    unchanged: bool,
}

/// Sync a single person's own data: localized name + biography and ranked profile
/// images. Visits the person's remotes in priority order (mirroring
/// [`sync_movie`]); only TMDB supplies data, with a per-remote [`api::RemoteCache`]
/// (conditional ETag + per-sub-request error suppression). Scheduled by the
/// background poller independently of shows/movies.
#[tracing::instrument(skip_all, fields(%person_id))]
pub(crate) async fn sync_person(
    person_id: api::PersonId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
    shutdown: &Shutdown,
) -> Result<()> {
    let Some(person) = db.person_by_id(None, person_id).await? else {
        return Ok(());
    };

    let config = db.load_config().await?;
    let now = api::Timestamp::now();

    let mut entries = person
        .remotes
        .iter()
        .filter(|e| e.enabled)
        .collect::<Vec<_>>();

    entries.sort_by_key(|e| e.priority);

    let mut draft = PersonDraft::default();
    let mut cache_writes: Vec<(api::RemoteId, CacheState)> = Vec::new();
    let mut seen = HashSet::new();
    let mut errored = false;

    for entry in entries {
        if shutdown.is_cancelled() {
            break;
        }

        let source = *entry.remote.source();

        if !seen.insert(source) {
            continue;
        }

        // A person is a single sync unit represented as Base; only a source
        // configured for it, and only TMDB (the sole person-data provider),
        // actually fetches. Others are reference-only links.
        let configured = api::effective_remote_sync_kinds(entry, &config);

        if !configured.contains(SyncKind::Base) || source != RemoteSource::Tmdb {
            continue;
        }

        let Some(tmdb_id) = entry.remote.value().as_u32() else {
            continue;
        };

        // Reuse a validator only until a higher-priority layer already produced data.
        let allow_skip = !draft.updated;
        let mut state = CacheState::new(entry.cache.as_ref(), now);

        let result = tmdb_person_layer(
            &mut draft, &mut state, &config, tmdb_id, remote, shutdown, allow_skip,
        )
        .await;

        cache_writes.push((entry.id, state));

        match result {
            Ok(LayerOutcome::Updated) => draft.updated = true,
            Ok(LayerOutcome::Unchanged) => draft.unchanged = true,
            Ok(LayerOutcome::Absent) => {}
            Err(e) => {
                errored = true;
                tracing::warn!(?source, "Sync layer failed for person {person_id}: {e:#}");
            }
        }
    }

    if shutdown.is_cancelled() {
        anyhow::bail!("Sync aborted: service is shutting down");
    }

    // Persist fetched data only when a layer produced a fresh draft. A 304 keeps
    // the stored data; a transient error leaves `last_synced_at` untouched so the
    // poller retries next cycle rather than parking the person for the full interval.
    let persisted = draft.updated;

    if persisted {
        db.persist_person_sync(
            person_id,
            draft.department,
            draft.default_language,
            draft.strings,
            draft.images,
            now,
        )
        .await?;

        // Store remotes discovered from the source (e.g. IMDb). `add_remote`
        // upserts, so re-running a sync is idempotent.
        for (slug, remote) in &draft.remotes {
            db.add_remote(person_id, slug.as_deref(), remote).await?;
        }
    } else if !errored {
        db.mark_person_synced(person_id, now).await?;
    }

    for (remote_id, state) in &cache_writes {
        db.set_remote_cache(person_id, *remote_id, cache_json(&state.finish(persisted)))
            .await?;
    }

    broadcast.broadcast_event(api::AppEventKind::PersonChanged { person_id });
    Ok(())
}

/// The TMDB person layer: fetch the person detail conditionally (ETag), then the
/// translations and images sub-requests (each recovered independently), building a
/// localized name/biography + ranked profiles into the draft. Returns
/// [`LayerOutcome::Unchanged`] on a 304 (the person resource, and therefore its
/// translations/images, are unchanged).
#[tracing::instrument(skip_all, fields(tmdb_id))]
async fn tmdb_person_layer(
    draft: &mut PersonDraft,
    state: &mut CacheState,
    config: &api::Config,
    tmdb_id: u32,
    remote: &RemoteClients,
    shutdown: &Shutdown,
    allow_skip: bool,
) -> Result<LayerOutcome> {
    let needed = needed_kinds(true, false, false);

    let etag = state
        .prior
        .as_ref()
        .filter(|c| usable_cache(c, needed, allow_skip, state.now))
        .and_then(|c| c.etag.clone());

    let (fresh_etag, info) = match remote.fetch_tmdb_person(tmdb_id, etag.as_deref()).await? {
        tmdb::Conditional::NotModified => {
            tracing::info!(tmdb_id, "Person unchanged (ETag 304)");
            return Ok(LayerOutcome::Unchanged);
        }
        tmdb::Conditional::Modified { etag, value } => (etag, value),
    };

    if shutdown.is_cancelled() {
        anyhow::bail!("Sync aborted: service is shutting down");
    }

    draft.department = info.department.clone();

    // Update the TMDB remote's slug from the person's name so external links resolve
    // to the canonical `/person/{id}-{slug}` URL (TMDB doesn't return a slug).
    let slug = info.name.as_deref().map(slugify).filter(|s| !s.is_empty());
    draft.remotes.push((slug, api::Remote::tmdb(tmdb_id)));

    // TMDB hands back the person's IMDb id; carry it as a proper IMDb remote so it
    // appears in the person's remotes list, mirroring show/movie remote discovery.
    if let Some(imdb_id) = info.imdb_id.as_deref().filter(|s| !s.is_empty()) {
        draft.remotes.push((None, api::Remote::imdb(imdb_id)));
    }

    // Translations (localized name/biography) and images are separate endpoints;
    // recover each so one failing doesn't sink the layer, and it retries with a TTL.
    let translations = recover(
        state,
        translations_key(),
        remote.fetch_tmdb_person_translations(tmdb_id),
    )
    .await
    .unwrap_or_default();

    if let Some(images) = recover(
        state,
        person_images_key(),
        remote.fetch_tmdb_person_images(tmdb_id),
    )
    .await
    {
        draft.images = images;
    }

    // Resolve sync targets against the person's canonical (primary) language,
    // which also becomes the display fallback.
    let primary = translations
        .iter()
        .find(|t| t.primary)
        .map(|t| t.locale)
        .unwrap_or_default();

    draft.default_language = primary;

    let targets = api::expand_sync_languages(&config.sync_languages, primary);
    let mut remaining = targets.clone();

    for t in &translations {
        let Some(target) = locale_matches_targets(t.locale, &targets) else {
            continue;
        };

        remaining.remove(&target);

        push_string(
            &mut draft.strings,
            t.locale,
            api::StringKind::Title,
            t.name.clone().or_else(|| info.name.clone()),
        );
        push_string(
            &mut draft.strings,
            t.locale,
            api::StringKind::Overview,
            t.biography.clone().or_else(|| info.biography.clone()),
        );
    }

    // Targets with no translation fall back to the detail (default-language) data.
    for locale in remaining {
        push_string(
            &mut draft.strings,
            locale,
            api::StringKind::Title,
            info.name.clone(),
        );
        push_string(
            &mut draft.strings,
            locale,
            api::StringKind::Overview,
            info.biography.clone(),
        );
    }

    state.earn(api::RemoteCache {
        etag: fresh_etag,
        last_updated: None,
        kinds: needed,
        errors: Vec::new(),
    });

    Ok(LayerOutcome::Updated)
}

/// Ensure the person behind a credit exists, seeding a placeholder name and
/// profile from the credit response only while the person has never been synced
/// (so the cast grid is populated before the person's own sync runs). Returns the
/// stable [`api::PersonId`].
fn seed_person(s: &mut InnerWrite, credit: &CreditDraft) -> Result<api::PersonId> {
    let (person_id, last_synced) = s.upsert_person(credit.source, credit.remote_person_id)?;

    if last_synced.is_none() {
        for (locale, name) in &credit.names {
            s.seed_person_string(person_id, *locale, api::StringKind::Title, name)?;
        }

        // Anchor the display language to a locale we actually seeded a name under,
        // so the name resolves on the person page/list before the person's own sync
        // runs. The person's original language isn't known from the credit response
        // and may not even be among the fetched locales, so it can't be used here.
        if let Some((locale, _)) = credit.names.first() {
            s.seed_person_default_language(person_id, *locale)?;
        }

        if let Some(profile) = &credit.profile {
            s.seed_person_image(person_id, ImageKind::Profile, profile)?;
        }
    }

    Ok(person_id)
}

/// Clear and rebuild a show's credits from the draft, then prune orphaned people.
fn persist_show_credits(
    s: &mut InnerWrite,
    show_id: api::ShowId,
    credits: &[CreditDraft],
) -> Result<()> {
    s.clear_show_credits(show_id)?;

    for credit in credits {
        let person_id = seed_person(s, credit)?;
        let credit_id = api::CreditId::random();

        s.insert_show_credit(
            credit_id,
            show_id,
            person_id,
            credit.kind,
            credit.department.as_deref(),
            credit.job.as_deref(),
            credit.order,
            credit.episode_count,
        )?;

        for (locale, character) in &credit.characters {
            s.insert_show_credit_string(credit_id, *locale, api::StringKind::Character, character)?;
        }
    }

    s.prune_orphan_people()?;
    Ok(())
}

/// Write the draft in one transaction, so a failure leaves the stored show
/// untouched and no image selection made meanwhile is lost, then announce it.
async fn persist_show_draft(
    show_id: api::ShowId,
    show: &api::Show,
    draft: &Arc<ShowDraft>,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    let first_air = draft.first_air_date.or(show.first_air_date);
    let draft = Arc::clone(draft);

    db.transaction(move |s| write_show_draft(s, show_id, first_air, &draft))
        .await?;

    let updated = db
        .show_by_id(None, show_id)
        .await?
        .context("Expected show to exist after update")?;
    broadcast.broadcast_event(api::AppEventKind::ShowChanged { show: updated });

    let seasons = db.seasons(None, show_id).await?;
    broadcast.broadcast_event(api::AppEventKind::SeasonsChanged { show_id, seasons });

    broadcast.broadcast_event(api::AppEventKind::TranslationsChanged {
        target: api::TranslationTarget::Show(show_id),
    });

    broadcast.broadcast_event(api::AppEventKind::CreditsChanged {
        target: api::TranslationTarget::Show(show_id),
    });

    Ok(())
}

fn write_show_draft(
    s: &mut InnerWrite,
    show_id: api::ShowId,
    first_air: Option<api::Timestamp>,
    draft: &ShowDraft,
) -> Result<()> {
    s.update_show(show_id, first_air)?;

    if !draft.original_language.is_default() {
        s.set_show_default_language(show_id, draft.original_language)?;
    }

    s.replace_show_strings(show_id, draft.show_strings.clone())?;

    for (slug, remote) in &draft.remotes {
        s.add_remote(show_id, slug.as_deref(), remote)?;
    }

    // Graphics: replace all show images with the accumulated set, ranked in
    // source-priority (accumulation) order. Preserve the user's explicit pick
    // per kind if that image still exists; otherwise fall back to the
    // highest-priority default.
    let preserved = s.user_selected_show_image_keys(show_id)?;

    s.clear_show_images(show_id)?;

    let mut ranks: HashMap<ImageKind, u32> = HashMap::new();
    let mut user_ids: HashMap<ImageKind, ImageId> = HashMap::new();
    let mut default_ids: HashMap<ImageKind, ImageId> = HashMap::new();

    for draft_image in &draft.images {
        let id = ImageId::random();
        let rank = ranks.entry(draft_image.kind).or_default();

        s.upsert_show_image(
            id,
            show_id,
            draft_image.kind,
            *rank,
            &draft_image.image,
            Some(draft_image.score),
        )?;
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
            s.set_show_image_selection(show_id, kind, id, true)?;
        } else if let Some(&id) = default_ids.get(&kind) {
            s.set_show_image_selection(show_id, kind, id, false)?;
        }
    }

    // Episodes: assign stable ids (reuse existing) so air-date releases
    // attribute to the right row.
    let existing_episode_ids = s.episode_ids(show_id)?;

    s.clear_episode_images(show_id)?;

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

        s.upsert_episode(
            episode_id,
            show_id,
            *season,
            *number,
            ep.absolute_number,
            ep.aired,
        )?;

        let strings = draft
            .episode_strings
            .get(&(*season, *number))
            .cloned()
            .unwrap_or_default();
        s.replace_episode_strings(episode_id, strings)?;

        if let Some(screenshot) = &ep.screenshot {
            let image_id = ImageId::random();
            s.upsert_episode_image(image_id, episode_id, ImageKind::Screenshot, screenshot)?;
            s.set_episode_image_selection(episode_id, ImageKind::Screenshot, image_id)?;
        }
    }

    // Seasons.
    let mut synced_seasons = HashSet::new();

    for (number, season) in &draft.seasons {
        let season_id = s.upsert_season(show_id, *number, season.air_date)?;

        let strings = draft
            .season_strings
            .get(number)
            .cloned()
            .unwrap_or_default();
        s.replace_season_strings(season_id, strings)?;

        s.clear_season_images(season_id)?;

        if let Some(poster) = &season.poster {
            let image_id = ImageId::random();
            s.upsert_season_image(image_id, season_id, ImageKind::Poster, poster)?;
            s.set_season_image_selection(season_id, ImageKind::Poster, image_id)?;
        }

        synced_seasons.insert(*number);
    }

    // Pruning treats absence from the draft as evidence of removal upstream. That only
    // holds if every layer reported everything it has: when one recovered from a failed
    // sub-request (a season whose episodes 5xx'd, say), those episodes are missing from
    // the draft because we never fetched them, not because they are gone. Pruning then
    // would delete real data over a transient blip, so a degraded run prunes nothing and
    // waits for a clean one.
    if draft.degraded_sources.is_empty() {
        for (season, kept) in &season_episode_numbers {
            s.prune_season_episodes(show_id, *season, kept)?;
        }

        s.prune_seasons(show_id, &synced_seasons)?;
    } else {
        tracing::warn!(
            sources = ?draft.degraded_sources,
            "Skipping prune: a sync layer recovered from a failed sub-request, so the draft is incomplete"
        );
    }

    // Air-date releases, attributed per source; skip episodes we didn't persist.
    // Track what we wrote so stale releases can be pruned afterwards, scoped to the
    // sources whose air-date layer actually ran this sync (`draft.air_date_sources`).
    let mut kept_releases = HashSet::new();

    for r in &draft.releases {
        let Some(&episode_id) = episode_ids.get(&(r.season, r.number)) else {
            continue;
        };

        s.upsert_episode_release(episode_id, r.source, r.country, &r.network, r.timestamp)?;

        kept_releases.insert((episode_id, r.source, r.country, r.network.clone()));
    }

    s.prune_episode_releases(show_id, &kept_releases, &draft.air_date_sources)?;

    // Credits: rebuild from the draft. Skip on a degraded run so a transient
    // credit-fetch failure doesn't wipe stored credits.
    if draft.degraded_sources.is_empty() {
        persist_show_credits(s, show_id, &draft.credits)?;
    }

    Ok(())
}

/// Persist only air-date releases when the Base layer was unchanged (cache hit):
/// the stored seasons/episodes/strings are kept, and we just upsert the releases
/// other sources produced this run (attributed to existing episodes) and prune
/// stale ones, scoped to the sources that actually ran. The caller's downstream
/// `recompute_episode_aired_for_show` + `EpisodesChanged` broadcasts surface any
/// resulting change.
fn persist_air_dates_only(
    show_id: api::ShowId,
    draft: &ShowDraft,
    s: &mut InnerWrite,
) -> Result<()> {
    // Even when the Base layer is unchanged, a non-base layer (e.g. TVDB running
    // only to accumulate graphics) may have discovered remote metadata such as a
    // slug that external links depend on. `add_remote` only fills in the
    // slug on conflict, so persisting the accumulated remotes here is idempotent.
    for (slug, remote) in &draft.remotes {
        s.add_remote(show_id, slug.as_deref(), remote)?;
    }

    // The base-unchanged path keeps the base source's stored images (the base
    // layer reported no fresh data). But a lower-priority source that ran this
    // sync - e.g. a newly-enabled TVDB - did re-supply its graphics, and those
    // would never be written otherwise. Refresh just those sources' images,
    // appending them after the retained (higher-priority base) images per kind,
    // and re-attach any user pick that pointed at a replaced image.
    if !draft.graphics_sources.is_empty() {
        let preserved = s.user_selected_show_image_keys(show_id)?;

        for source in &draft.graphics_sources {
            s.delete_show_images_for_source(show_id, image_source(*source))?;
        }

        let mut ranks = s.next_show_image_ranks(show_id)?;
        let mut user_ids: HashMap<ImageKind, ImageId> = HashMap::new();
        let mut default_ids: HashMap<ImageKind, ImageId> = HashMap::new();

        for draft_image in &draft.images {
            let id = ImageId::random();
            let rank = ranks.entry(draft_image.kind).or_default();

            s.upsert_show_image(
                id,
                show_id,
                draft_image.kind,
                *rank,
                &draft_image.image,
                Some(draft_image.score),
            )?;
            *rank += 1;

            if preserved.get(&draft_image.kind) == Some(draft_image.image.key()) {
                user_ids.entry(draft_image.kind).or_insert(id);
            }

            if draft.selected.get(&draft_image.kind) == Some(draft_image.image.key()) {
                default_ids.entry(draft_image.kind).or_insert(id);
            }
        }

        for (kind, id) in user_ids {
            s.set_show_image_selection(show_id, kind, id, true)?;
        }

        // Re-attach a default only for a kind whose selection was cascaded away
        // with a replaced image (leaving it unselected). Kinds still selected by
        // a surviving higher-priority source - or a user pick just set - keep it.
        let selected = s.show_selected_image_kinds(show_id)?;

        for (kind, id) in default_ids {
            if !selected.contains(&kind) {
                s.set_show_image_selection(show_id, kind, id, false)?;
            }
        }
    }

    let episode_ids = s.episode_ids(show_id)?;
    let mut kept_releases = HashSet::new();

    for r in &draft.releases {
        let Some(&episode_id) = episode_ids.get(&(r.season, r.number)) else {
            continue;
        };

        s.upsert_episode_release(episode_id, r.source, r.country, &r.network, r.timestamp)?;

        kept_releases.insert((episode_id, r.source, r.country, r.network.clone()));
    }

    s.prune_episode_releases(show_id, &kept_releases, &draft.air_date_sources)?;

    Ok(())
}

/// The shared model the single-episode sync layers contribute to: the episode-scoped
/// analog of [`ShowDraft`]. Contributions are governed by the same rules - `provided`
/// makes the exclusive Base kind the property of the highest-priority source, while
/// Dates accumulate from every source.
#[derive(Default)]
struct EpisodeDraftModel {
    provided: SyncKindSet,
    /// The Base layer reported its episode unchanged (ETag 304 / equal
    /// `lastUpdated`), so the stored row, strings and screenshot are kept and
    /// only other sources' air dates are persisted.
    base_unchanged: bool,
    /// Validators captured by Updated layers, keyed by source, flushed to
    /// `episode_cache` only after a successful persist - so a stored validator
    /// always has its data behind it.
    cache_writes: Vec<(RemoteSource, CacheState)>,
    absolute_number: Option<u32>,
    aired: Option<api::Timestamp>,
    screenshot: Option<Image>,
    strings: StringRows,
    releases: Vec<api::EpisodeRelease>,
    /// Sources whose air-date layer ran this sync; scopes release pruning
    /// exactly as [`ShowDraft::air_date_sources`] does.
    air_date_sources: HashSet<RemoteSource>,
    /// Sources that recovered from a failed sub-request, mirroring
    /// [`ShowDraft::degraded_sources`].
    degraded_sources: HashSet<RemoteSource>,
}

impl EpisodeDraftModel {
    fn needs(&self, kind: SyncKind) -> bool {
        !kind.is_exclusive() || !self.provided.contains(kind)
    }
}

/// Sync a single episode. Every remote is addressed through the show's own remote id
/// plus the episode's `(season, number)` - an episode stores no remote id of its own.
///
/// Scheduled hourly around an episode's air date (see [`crate::background`]), because
/// remotes tend to correct episode metadata right around broadcast, and a full show
/// sync is far too expensive to run that often.
pub(crate) async fn sync_episode(
    show_id: api::ShowId,
    episode_id: EpisodeId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
    pending: &crate::pending::PendingSystem,
    shutdown: &Shutdown,
) -> Result<()> {
    let show = db
        .show_by_id(None, show_id)
        .await?
        .context("Expected show to exist")?;

    let episode = db
        .episode_by_id(None, episode_id)
        .await?
        .context("Expected episode to exist")?;

    let mut config = db.load_config().await?;
    config.sync_languages = api::sync_languages_for_viewers(
        &config.sync_languages,
        &db.show_viewer_languages(show_id).await?,
    );
    let cache = db.episode_cache(episode_id).await?;

    // One clock for the whole sync, so every error recorded this run expires together.
    let now = api::Timestamp::now();

    let season = episode.season;
    let number = episode.episode;

    tracing::info!(show_id = %show_id, code = %episode.code(), "Syncing episode");

    // Visit enabled remotes in priority order, one layer per source - the same
    // layering `sync_show` uses, so a re-prioritized remote takes over Base here too.
    let mut entries = show
        .remotes
        .iter()
        .filter(|e| e.enabled)
        .collect::<Vec<_>>();

    entries.sort_by_key(|e| e.priority);

    let mut draft = EpisodeDraftModel::default();
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

        // Unlike a show sync there are no per-source graphics to accumulate: an
        // episode's only image comes from its Base provider. So a source that owes
        // nothing has nothing to do.
        if kinds.is_empty() {
            continue;
        }

        let do_base = kinds.contains(SyncKind::Base);
        let do_air_date = kinds.contains(SyncKind::Dates);

        let allow_skip = do_base || draft.base_unchanged;

        let mut state = CacheState::new(cache.get(&source), now);

        let result = match source {
            RemoteSource::Tmdb => match entry.remote.value().as_u32() {
                Some(tmdb_id) => {
                    tmdb_episode_layer(
                        &mut draft,
                        &mut state,
                        &config,
                        &show,
                        tmdb_id,
                        season,
                        number,
                        do_base,
                        do_air_date,
                        remote,
                        allow_skip,
                    )
                    .await
                }
                None => continue,
            },
            RemoteSource::Tvdb => match entry.remote.value().as_u32() {
                Some(tvdb_id) => {
                    tvdb_episode_layer(
                        &mut draft,
                        &mut state,
                        &config,
                        &show,
                        tvdb_id,
                        season,
                        number,
                        do_base,
                        do_air_date,
                        remote,
                        allow_skip,
                        shutdown,
                    )
                    .await
                }
                None => continue,
            },
            RemoteSource::Tvmaze if do_air_date => match entry.remote.value().as_u32() {
                // TVmaze offers no conditional request, so it never earns a validator.
                Some(tvmaze_id) => {
                    tvmaze_episode_layer(&mut draft, &mut state, tvmaze_id, season, number, remote)
                        .await
                        .map(|()| LayerOutcome::Updated)
                }
                None => continue,
            },
            _ => continue,
        };

        // Recorded whether the layer succeeded or not: a validator only if it earned
        // one, but its errors either way - so an episode a source simply doesn't carry
        // stops costing a call on every sync.
        let degraded = state.degraded();

        if degraded {
            draft.degraded_sources.insert(source);
        }

        draft.cache_writes.push((source, state));

        // A failing layer must not abort the sync: what other layers collected still
        // persists, and the kind stays unclaimed so a lower-priority layer can fill it.
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(e) => {
                tracing::warn!(?source, "Sync layer failed for episode {episode_id}: {e:#}");
                continue;
            }
        };

        match outcome {
            // This source doesn't carry the episode. It claims nothing, so a
            // lower-priority source still gets to provide it - and the miss is now
            // cached, so we stop asking every sync.
            LayerOutcome::Absent => continue,
            LayerOutcome::Unchanged => {
                if do_base {
                    draft.base_unchanged = true;
                }
            }
            LayerOutcome::Updated => {
                if do_air_date && !degraded {
                    draft.air_date_sources.insert(source);
                }
            }
        }

        for k in kinds {
            draft.provided.insert(k);
        }
    }

    if shutdown.is_cancelled() {
        anyhow::bail!("Sync aborted: service is shutting down");
    }

    let eligible = api::eligible_sync_kinds(&show.remotes, &config);

    let persisted = draft.provided.contains(SyncKind::Base) && !draft.base_unchanged;
    let draft = Arc::new(draft);

    if persisted {
        let base = Arc::clone(&draft);
        db.transaction(move |s| {
            persist_episode_draft(show_id, episode_id, season, number, &base, s)
        })
        .await?;
    } else if !draft.base_unchanged
        && eligible.contains(SyncKind::Base)
        && draft.degraded_sources.is_empty()
    {
        // No Base source produced anything, and none of them reported *why* - so this
        // is a real fault (a misconfigured remote, an unreachable API) rather than the
        // remotes simply not carrying this episode. That distinction matters: an
        // episode a source doesn't have is an ordinary fact, recorded and cached, and
        // must not fail the task.
        anyhow::bail!("Episode has no syncable Base remote available");
    } else if !draft.degraded_sources.is_empty() {
        tracing::info!(
            sources = ?draft.degraded_sources,
            "No Base source carries this episode; keeping stored data"
        );
    }

    let releases = Arc::clone(&draft);
    db.transaction(move |s| persist_episode_releases(show_id, episode_id, &releases, s))
        .await?;

    // Written even when nothing above persisted: these carry the recorded failures, and
    // caching those is exactly what stops a missing episode being re-probed every sync.
    for (source, state) in &draft.cache_writes {
        db.set_episode_cache(episode_id, *source, cache_json(&state.finish(persisted)))
            .await?;
    }

    // Merge every source's air dates into the effective `aired` by priority. This is
    // a DB-local recompute over the show's stored releases, so scoping it to the one
    // episode would buy nothing.
    db.recompute_episode_aired_for_show(show_id, config.air_date_filters.clone())
        .await?;

    pending.fill_for_show(show_id, now).await?;
    db.set_episode_synced_at(episode_id, now).await?;

    if let Some(episode) = db.episode_by_id(None, episode_id).await? {
        broadcast.broadcast_event(api::AppEventKind::EpisodeChanged { episode });
    }

    broadcast.broadcast_event(api::AppEventKind::EpisodesChanged { show_id, season });
    broadcast.broadcast_event(api::AppEventKind::PendingChanged);

    tracing::info!(show_id = %show_id, code = %episode.code(), "Episode sync complete");
    Ok(())
}

/// The target locales an episode's strings are collected for: the configured
/// sync languages (with viewers' languages added) expanded against the show's
/// original language.
fn episode_string_targets(show: &api::Show, config: &api::Config) -> BTreeSet<api::Locale> {
    api::expand_sync_languages(&config.sync_languages, show.strings.locale())
}

#[tracing::instrument(skip_all, fields(tmdb_id, ?season, number, do_base, do_air_date))]
#[allow(clippy::too_many_arguments)]
async fn tmdb_episode_layer(
    draft: &mut EpisodeDraftModel,
    state: &mut CacheState,
    config: &api::Config,
    show: &api::Show,
    tmdb_id: u32,
    season: SeasonNumber,
    number: u32,
    do_base: bool,
    do_air_date: bool,
    remote: &RemoteClients,
    allow_skip: bool,
) -> Result<LayerOutcome> {
    let needed = needed_kinds(do_base, do_air_date, false);

    // Only replay the ETag when the cached validator already covers what we owe: a
    // 304 has no body, so an uncovered kind must force a full response.
    let etag = state
        .prior
        .as_ref()
        .filter(|c| usable_cache(c, needed, allow_skip, state.now))
        .and_then(|c| c.etag.clone());

    // The episode itself: a source that simply doesn't carry it answers `404`. That is
    // an ordinary fact about the remote, not a sync failure - record it (so it isn't
    // re-probed every run) and let a lower-priority source provide the episode instead.
    let key = episode_key(season, number);

    let Some(conditional) = recover(
        state,
        &key,
        remote.fetch_tmdb_episode(tmdb_id, season, number, etag.as_deref()),
    )
    .await
    else {
        return Ok(LayerOutcome::Absent);
    };

    let (fresh_etag, info) = match conditional {
        tmdb::Conditional::NotModified => {
            tracing::info!(tmdb_id, "Episode unchanged (ETag 304)");
            return Ok(LayerOutcome::Unchanged);
        }
        tmdb::Conditional::Modified { etag, value } => (etag, value),
    };

    if do_base {
        draft.aired = info.aired;
        draft.screenshot = info.filename.clone().map(Image::from);

        let targets = episode_string_targets(show, config);

        let translations = recover(
            state,
            &episode_translations_key(season, number),
            remote.fetch_tmdb_episode_translations(tmdb_id, season, number),
        )
        .await
        .unwrap_or_default();

        for translation in translations {
            if locale_matches_targets(translation.locale, &targets).is_none() {
                continue;
            }

            push_string(
                &mut draft.strings,
                translation.locale,
                api::StringKind::Title,
                translation.name.or_else(|| {
                    original_fallback(
                        translation.locale,
                        show.strings.locale(),
                        &info.original_name,
                    )
                }),
            );

            push_string(
                &mut draft.strings,
                translation.locale,
                api::StringKind::Overview,
                translation.overview,
            );
        }
    }

    if do_air_date && let Some(aired) = info.aired {
        draft.releases.push(api::EpisodeRelease {
            source: RemoteSource::Tmdb,
            country: api::Country::DEFAULT,
            network: String::new(),
            timestamp: aired,
        });
    }

    if let Some(earned) = (!needed.is_empty()).then_some(api::RemoteCache {
        etag: fresh_etag,
        last_updated: None,
        kinds: needed,
        errors: Vec::new(),
    }) {
        state.earn(earned);
    }

    Ok(LayerOutcome::Updated)
}

#[tracing::instrument(skip_all, fields(tvdb_id, ?season, number, do_base, do_air_date))]
#[allow(clippy::too_many_arguments)]
async fn tvdb_episode_layer(
    draft: &mut EpisodeDraftModel,
    state: &mut CacheState,
    config: &api::Config,
    show: &api::Show,
    tvdb_id: u32,
    season: SeasonNumber,
    number: u32,
    do_base: bool,
    do_air_date: bool,
    remote: &RemoteClients,
    allow_skip: bool,
    shutdown: &Shutdown,
) -> Result<LayerOutcome> {
    let needed = needed_kinds(do_base, do_air_date, false);

    let key = episode_key(season, number);

    let Some(found) = recover(
        state,
        &key,
        remote.fetch_tvdb_episode(tvdb_id, season, number),
    )
    .await
    else {
        return Ok(LayerOutcome::Absent);
    };

    // TVDB reports an episode it doesn't carry as an empty result rather than a `404`.
    // Same fact, so record it the same way: the miss is cached and not re-probed.
    let Some(info) = found else {
        let code = api::Code::new(season, number);
        state.record_missing(&key, format!("TVDB has no {code} for series {tvdb_id}"));
        return Ok(LayerOutcome::Absent);
    };

    // TVDB has no ETag; the record-level `lastUpdated` marker detects an unchanged
    // episode. Unlike TMDB's 304 the response body is already paid for, but an equal
    // marker still saves the per-language translation fetches.
    if let Some(cache) = state.prior.as_ref()
        && usable_cache(cache, needed, allow_skip, state.now)
        && let Some(cached) = cache.last_updated.as_deref()
        && let Some(current) = info.last_updated.as_deref()
        && cached == current
    {
        tracing::info!(tvdb_id, "Episode unchanged (lastUpdated)");
        return Ok(LayerOutcome::Unchanged);
    }

    if do_base {
        draft.aired = info.aired;
        draft.absolute_number = info.absolute_number;
        draft.screenshot = info
            .image
            .as_ref()
            .map(|(source, path)| Image::new(*source, path));

        let available = info
            .name_translations
            .iter()
            .chain(info.overview_translations.iter())
            .map(|n| n.to_lowercase())
            .collect::<HashSet<_>>();

        // TVDB has no country dimension, so collapse each target to its language.
        let targets: BTreeSet<api::Locale> = episode_string_targets(show, config)
            .into_iter()
            .map(|l| api::Locale::new(l.language(), api::Country::DEFAULT))
            .collect();

        for language in targets {
            if shutdown.is_cancelled() {
                anyhow::bail!("Sync aborted: service is shutting down");
            }

            // Remotes key on ISO 639-1; skip any locale with no 2-letter form.
            if language.language().to_part1().is_none() {
                continue;
            }

            let translation = recover(
                state,
                &format!(
                    "episode/{}/translations/{language}",
                    api::Code::new(season, number)
                ),
                remote.fetch_tvdb_episode_translation(info.id, language, &available),
            )
            .await
            .flatten();

            if let Some(translation) = translation {
                push_string(
                    &mut draft.strings,
                    language,
                    api::StringKind::Title,
                    translation.name,
                );

                push_string(
                    &mut draft.strings,
                    language,
                    api::StringKind::Overview,
                    translation.overview,
                );
            }
        }
    }

    if do_air_date && let Some(aired) = info.aired {
        draft.releases.push(api::EpisodeRelease {
            source: RemoteSource::Tvdb,
            country: api::Country::DEFAULT,
            network: String::new(),
            timestamp: aired,
        });
    }

    if let Some(earned) = (!needed.is_empty()).then(|| api::RemoteCache {
        etag: None,
        last_updated: info.last_updated.clone(),
        kinds: needed,
        errors: Vec::new(),
    }) {
        state.earn(earned);
    }

    Ok(LayerOutcome::Updated)
}

/// TVmaze contributes air dates only, accumulated with the show's network/country.
#[tracing::instrument(skip_all, fields(tvmaze_id, ?season, number))]
async fn tvmaze_episode_layer(
    draft: &mut EpisodeDraftModel,
    state: &mut CacheState,
    tvmaze_id: u32,
    season: SeasonNumber,
    number: u32,
    remote: &RemoteClients,
) -> Result<()> {
    let key = episode_key(season, number);

    let Some(found) = recover(
        state,
        &key,
        remote.fetch_tvmaze_episode(tvmaze_id, season, number),
    )
    .await
    else {
        return Ok(());
    };

    let Some(info) = found else {
        let code = api::Code::new(season, number);
        state.record_missing(&key, format!("TVmaze has no {code} for show {tvmaze_id}"));
        return Ok(());
    };

    let network = recover(
        state,
        "network",
        remote.fetch_tvmaze_show_network(tvmaze_id),
    )
    .await
    .unwrap_or_default();

    draft.releases.push(api::EpisodeRelease {
        source: RemoteSource::Tvmaze,
        country: network.country,
        network: network.network,
        timestamp: info.aired_at,
    });

    Ok(())
}

/// Write the Base layer's contribution: the episode row, its strings, and its
/// screenshot. Releases are persisted separately, since they are written on the
/// base-unchanged path too.
fn persist_episode_draft(
    show_id: api::ShowId,
    episode_id: EpisodeId,
    season: SeasonNumber,
    number: u32,
    draft: &EpisodeDraftModel,
    s: &mut InnerWrite,
) -> Result<()> {
    s.upsert_episode(
        episode_id,
        show_id,
        season,
        number,
        draft.absolute_number,
        draft.aired,
    )?;

    s.replace_episode_strings(episode_id, draft.strings.clone())?;

    // Scoped to this episode: the show-wide image clear would drop every other
    // episode's screenshot.
    s.clear_images_for_episode(episode_id)?;

    if let Some(screenshot) = &draft.screenshot {
        let image_id = ImageId::random();
        s.upsert_episode_image(image_id, episode_id, ImageKind::Screenshot, screenshot)?;
        s.set_episode_image_selection(episode_id, ImageKind::Screenshot, image_id)?;
    }

    Ok(())
}

/// Upsert the draft's air dates and prune the stale ones, scoped to this episode and
/// to the sources whose layer actually ran.
fn persist_episode_releases(
    show_id: api::ShowId,
    episode_id: EpisodeId,
    draft: &EpisodeDraftModel,
    s: &mut InnerWrite,
) -> Result<()> {
    let mut kept = HashSet::new();

    for r in &draft.releases {
        s.upsert_episode_release(episode_id, r.source, r.country, &r.network, r.timestamp)?;

        kept.insert((r.source, r.country, r.network.clone()));
    }

    s.prune_episode_releases_for_episode(show_id, episode_id, &kept, &draft.air_date_sources)?;

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
    cache_writes: Vec<(api::RemoteId, CacheState)>,
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
    /// Sources that recovered from a failed sub-request, mirroring
    /// [`ShowDraft::degraded_sources`].
    degraded_sources: HashSet<RemoteSource>,
    strings: StringRows,
    /// Cast & crew, provided by the TMDB Credits layer.
    credits: Vec<CreditDraft>,
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
        .movie_by_id(None, movie_id)
        .await?
        .context("Expected movie to exist")?;

    let mut config = db.load_config().await?;
    config.sync_languages = api::sync_languages_for_viewers(
        &config.sync_languages,
        &db.movie_viewer_languages(movie_id).await?,
    );

    tracing::info!(movie_id = %movie_id, title = movie.strings.title(), "Syncing movie");

    // One clock for the whole sync, so every error recorded this run expires together.
    let now = api::Timestamp::now();

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
        let do_credits = kinds.contains(SyncKind::Credits);

        let allow_skip = do_base || draft.base_unchanged;

        let mut state = CacheState::new(entry.cache.as_ref(), now);

        let result = match source {
            RemoteSource::Tmdb => match entry.remote.value().as_u32() {
                Some(tmdb_id) => {
                    tmdb_movie_layer(
                        &mut draft, &mut state, &config, tmdb_id, do_base, do_release, do_credits,
                        remote, shutdown, allow_skip,
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

        let degraded = state.degraded();

        if degraded {
            draft.degraded_sources.insert(source);
        }

        draft.cache_writes.push((entry.id, state));

        let outcome = match result {
            Ok(outcome) => outcome,
            Err(e) => {
                tracing::warn!(?source, "Sync layer failed for movie {movie_id}: {e:#}");
                continue;
            }
        };

        match outcome {
            // Contributed nothing, so claims nothing - a lower-priority source may
            // still provide the kinds this one owed.
            LayerOutcome::Absent => continue,
            LayerOutcome::Unchanged => {
                if kinds.contains(SyncKind::Base) {
                    draft.base_unchanged = true;
                }
            }
            LayerOutcome::Updated => {
                if do_release && !degraded {
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

    let draft = Arc::new(draft);

    if draft.provided.contains(SyncKind::Base) && !draft.base_unchanged {
        let base = Arc::clone(&draft);
        db.transaction(move |s| persist_movie_draft(movie_id, &base, s))
            .await?;
        flush_movie_cache_writes(movie_id, &draft.cache_writes, db, true).await?;
    } else if draft.base_unchanged {
        // Base source unchanged (cache hit): keep stored metadata/strings/images,
        // persist only other sources' fresh releases.
        let releases = Arc::clone(&draft);
        db.transaction(move |s| persist_movie_releases_only(movie_id, &releases, s))
            .await?;
        flush_movie_cache_writes(movie_id, &draft.cache_writes, db, true).await?;
    } else if eligible.contains(SyncKind::Base) {
        flush_movie_cache_writes(movie_id, &draft.cache_writes, db, false).await?;

        // As in `sync_show`: a remote that explained itself (a `404`, an unreachable
        // endpoint) has had that recorded and cached, and must not fail the task.
        if draft.degraded_sources.is_empty() {
            anyhow::bail!("Movie has no syncable Base remote available");
        }

        tracing::warn!(
            sources = ?draft.degraded_sources,
            "No Base source produced data; keeping the stored movie"
        );
    } else {
        flush_movie_cache_writes(movie_id, &draft.cache_writes, db, false).await?;

        // No enabled remote contributes Base: the movie's derived metadata is
        // orphaned, so clear it (mirrors the show clear branch).
        db.transaction(move |s| {
            s.replace_movie_strings(movie_id, Vec::new())?;
            s.prune_movie_releases(movie_id, &HashSet::new(), &HashSet::new())
        })
        .await?;
    }

    // Recompute the effective release date + pending entry from the movie's release filters before
    // broadcasting, so the emitted movie reflects the filtered release date.
    db.update_movie_pending(movie_id, config.release_filters.clone())
        .await?;

    let updated = db
        .movie_by_id(None, movie_id)
        .await?
        .context("Expected movie to exist after update")?;

    broadcast.broadcast_event(api::AppEventKind::MovieChanged { movie: updated });

    broadcast.broadcast_event(api::AppEventKind::TranslationsChanged {
        target: api::TranslationTarget::Movie(movie_id),
    });

    broadcast.broadcast_event(api::AppEventKind::CreditsChanged {
        target: api::TranslationTarget::Movie(movie_id),
    });

    db.set_movie_synced_at(movie_id, api::Timestamp::now())
        .await?;
    broadcast.broadcast_event(api::AppEventKind::PendingChanged);
    tracing::info!(movie_id = %movie_id, "Sync complete");
    Ok(())
}

/// Flush what the movie layers learned. See [`flush_show_cache_writes`].
async fn flush_movie_cache_writes(
    movie_id: api::MovieId,
    writes: &[(api::RemoteId, CacheState)],
    db: &Database,
    persisted: bool,
) -> Result<()> {
    for (remote_id, state) in writes {
        db.set_remote_cache(movie_id, *remote_id, cache_json(&state.finish(persisted)))
            .await?;
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
    state: &mut CacheState,
    config: &api::Config,
    tmdb_id: u32,
    do_base: bool,
    do_release: bool,
    do_credits: bool,
    remote: &RemoteClients,
    shutdown: &Shutdown,
    allow_skip: bool,
) -> Result<LayerOutcome> {
    tracing::info!(tmdb_id, do_base, do_release, do_credits, "Movie");

    let needed = needed_kinds(do_base, do_release, do_credits);

    let etag = state
        .prior
        .as_ref()
        .filter(|c| usable_cache(c, needed, allow_skip, state.now))
        .and_then(|c| c.etag.clone());

    // A movie the remote no longer carries answers `404`; record it rather than
    // retrying on every sync.
    let Some(conditional) = recover(
        state,
        "movie",
        remote.fetch_tmdb_movie(tmdb_id, etag.as_deref()),
    )
    .await
    else {
        return Ok(LayerOutcome::Absent);
    };

    let (fresh_etag, info) = match conditional {
        tmdb::Conditional::NotModified => {
            tracing::info!(tmdb_id, "Movie unchanged (ETag 304)");
            return Ok(LayerOutcome::Unchanged);
        }
        tmdb::Conditional::Modified { etag, value } => (etag, value),
    };

    // Built here, handed to `state` only on the success path below.
    let earned = (!needed.is_empty()).then_some(api::RemoteCache {
        etag: fresh_etag,
        last_updated: None,
        kinds: needed,
        errors: Vec::new(),
    });

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

        if let Some(rows) = recover(
            state,
            translations_key(),
            collect_tmdb_movie_strings(tmdb_id, &info, config, remote, shutdown),
        )
        .await
        {
            draft.strings = rows;
        }
    }

    if do_credits
        && let Some(credits) = recover(
            state,
            credits_key(),
            collect_tmdb_movie_credits(tmdb_id, config, info.original_language, remote, shutdown),
        )
        .await
    {
        draft.credits = credits;
    }

    if do_release
        && let Some(releases) =
            recover(state, "releases", remote.fetch_tmdb_movie_releases(tmdb_id)).await
    {
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

    if let Some(earned) = earned {
        state.earn(earned);
    }

    Ok(LayerOutcome::Updated)
}

/// Write the accumulated [`MovieDraft`] in one pass: base language, strings,
/// discovered remotes, accumulated graphics (ranked, with selection) and
/// per-source releases; prune releases no longer reported.
fn persist_movie_draft(
    movie_id: api::MovieId,
    draft: &MovieDraft,
    s: &mut InnerWrite,
) -> Result<()> {
    if !draft.original_language.is_default() {
        s.set_movie_default_language(movie_id, draft.original_language)?;
    }

    s.replace_movie_strings(movie_id, draft.strings.clone())?;

    for (slug, remote) in &draft.remotes {
        s.add_remote(movie_id, slug.as_deref(), remote)?;
    }

    // Graphics: replace all movie images with the accumulated set, ranked in
    // source-priority order. Preserve the user's explicit pick per kind if that
    // image still exists; otherwise fall back to the highest-priority default.
    let preserved = s.user_selected_movie_image_keys(movie_id)?;

    s.clear_movie_images(movie_id)?;

    let mut ranks: HashMap<ImageKind, u32> = HashMap::new();
    let mut user_ids: HashMap<ImageKind, ImageId> = HashMap::new();
    let mut default_ids: HashMap<ImageKind, ImageId> = HashMap::new();

    for draft_image in &draft.images {
        let id = ImageId::random();
        let rank = ranks.entry(draft_image.kind).or_default();

        s.upsert_movie_image(
            id,
            movie_id,
            draft_image.kind,
            *rank,
            &draft_image.image,
            Some(draft_image.score),
        )?;
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
        s.set_movie_image_selection(movie_id, *kind, *id, *user_selected)?;
    }

    // A movie has no banner artwork of its own; mirror the backdrop selection so
    // banner slots display the backdrop (as the previous inline sync did).
    if let Some(&(id, user_selected)) = resolved.get(&ImageKind::Backdrop) {
        s.set_movie_image_selection(movie_id, ImageKind::Banner, id, user_selected)?;
    }

    persist_movie_releases(movie_id, draft, s)?;

    // Credits: rebuild from the draft. Skip on a degraded run so a transient
    // credit-fetch failure doesn't wipe stored credits.
    if draft.degraded_sources.is_empty() {
        persist_movie_credits(s, movie_id, &draft.credits)?;
    }

    Ok(())
}

/// Fetch cast & crew for a movie once per synced language and merge them.
#[tracing::instrument(skip_all, fields(tmdb_id))]
async fn collect_tmdb_movie_credits(
    tmdb_id: u32,
    config: &api::Config,
    original_language: api::Locale,
    remote: &RemoteClients,
    shutdown: &Shutdown,
) -> Result<Vec<CreditDraft>> {
    let targets = api::expand_sync_languages(&config.sync_languages, original_language);

    let mut merged: BTreeMap<String, CreditDraft> = BTreeMap::new();

    for locale in &targets {
        if shutdown.is_cancelled() {
            anyhow::bail!("Sync aborted: service is shutting down");
        }

        let credits = remote
            .fetch_tmdb_movie_credits(tmdb_id, &locale.to_string())
            .await?;

        merge_credits(&mut merged, credits, *locale);
    }

    Ok(merged.into_values().collect())
}

/// Clear and rebuild a movie's credits from the draft, then prune orphaned people.
fn persist_movie_credits(
    s: &mut InnerWrite,
    movie_id: api::MovieId,
    credits: &[CreditDraft],
) -> Result<()> {
    s.clear_movie_credits(movie_id)?;

    for credit in credits {
        let person_id = seed_person(s, credit)?;
        let credit_id = api::CreditId::random();

        s.insert_movie_credit(
            credit_id,
            movie_id,
            person_id,
            credit.kind,
            credit.department.as_deref(),
            credit.job.as_deref(),
            credit.order,
            credit.episode_count,
        )?;

        for (locale, character) in &credit.characters {
            s.insert_movie_credit_string(
                credit_id,
                *locale,
                api::StringKind::Character,
                character,
            )?;
        }
    }

    s.prune_orphan_people()?;
    Ok(())
}

/// Persist only the releases from a cache-hit movie sync: the base layer reported
/// unchanged, so stored metadata/strings/images are kept and only releases other
/// sources produced this run are written.
fn persist_movie_releases_only(
    movie_id: api::MovieId,
    draft: &MovieDraft,
    s: &mut InnerWrite,
) -> Result<()> {
    persist_movie_releases(movie_id, draft, s)
}

/// Upsert the draft's releases and prune stale ones, scoped to the sources whose
/// release layer ran this sync. Shared by the full and cache-hit persist paths.
fn persist_movie_releases(
    movie_id: api::MovieId,
    draft: &MovieDraft,
    s: &mut InnerWrite,
) -> Result<()> {
    let mut kept = HashSet::new();

    for r in &draft.releases {
        s.upsert_movie_release(movie_id, r.source, r.country, r.release_type, &r.timestamp)?;

        kept.insert((r.source, r.country, r.release_type));
    }

    s.prune_movie_releases(movie_id, &kept, &draft.release_sources)?;

    Ok(())
}

/// Fetch and replace a movie's per-language translated strings. Languages are
/// [`api::expand_sync_languages`] of the configured `sync_languages` against the
/// movie's own original language.
#[tracing::instrument(skip_all, fields(tmdb_id))]
async fn collect_tmdb_movie_strings(
    tmdb_id: u32,
    info: &tmdb::MovieInfo,
    config: &api::Config,
    remote: &RemoteClients,
    shutdown: &Shutdown,
) -> Result<StringRows> {
    let targets = api::expand_sync_languages(&config.sync_languages, info.original_language);

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

        tracing::info!(?translation, ?targets, "Movie translation");

        push_string(
            &mut rows,
            translation.locale,
            api::StringKind::Title,
            translation.title.or_else(|| {
                original_fallback(
                    translation.locale,
                    info.original_language,
                    &info.original_title,
                )
            }),
        );

        push_string(
            &mut rows,
            translation.locale,
            api::StringKind::Overview,
            translation.overview.or_else(|| {
                original_fallback(
                    translation.locale,
                    info.original_language,
                    &info.original_overview,
                )
            }),
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::OpenMode;

    fn draft(season: u32, poster: &str) -> ShowDraft {
        let number = SeasonNumber::from_ordinal(season);
        let mut draft = ShowDraft::default();

        draft.seasons.insert(
            number,
            SeasonDraft {
                tvdb_id: None,
                air_date: None,
                poster: None,
                tvdb_translations: Arc::default(),
            },
        );

        draft.episodes.insert(
            (number, 1),
            EpisodeDraft {
                tvdb_id: None,
                original_name: None,
                absolute_number: None,
                aired: None,
                screenshot: None,
                tvdb_translations: Arc::default(),
            },
        );

        draft.images.push(DraftImage {
            kind: ImageKind::Poster,
            image: Image::tmdb(poster),
            score: 1.0,
        });

        draft
            .selected
            .insert(ImageKind::Poster, Image::tmdb(poster).key().clone());

        draft
    }

    /// Every write of a show draft belongs to the caller's transaction: when
    /// it fails after the draft is written, the show keeps what it had.
    #[tokio::test]
    async fn show_draft_is_written_in_one_transaction() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = Database::open(dir.path().join("test.db"), OpenMode::Normal, 1)?;
        let show_id = api::ShowId::new(1);
        db.create_show(show_id, "Show", None, "").await?;

        let first = draft(1, "/first.jpg");
        db.transaction(move |s| write_show_draft(s, show_id, None, &first))
            .await?;

        let second = draft(2, "/second.jpg");
        let failed = db
            .transaction(move |s| -> Result<()> {
                write_show_draft(s, show_id, None, &second)?;
                anyhow::bail!("fails after the draft is written")
            })
            .await;

        assert!(failed.is_err());

        let seasons: Vec<_> = db
            .seasons(None, show_id)
            .await?
            .into_iter()
            .map(|s| s.season)
            .collect();
        assert_eq!(seasons, [SeasonNumber::from_ordinal(1)]);

        let show = db.show_by_id(None, show_id).await?.context("show")?;
        assert_eq!(
            show.poster.map(|p| p.key().clone()),
            Some(Image::tmdb("/first.jpg").key().clone())
        );
        assert_eq!(show.images.len(), 1);
        Ok(())
    }

    /// A configured language TMDB has no translation for, or whose entry has no
    /// name, is left missing; only the original language takes the original name.
    #[test]
    fn tmdb_show_titles_fall_back_only_in_original_language() {
        let locale = |iso| api::Locale::from_iso(iso).unwrap();

        let translation = |iso_639_1, iso_3166_1, name: Option<&str>| tmdb::Translation {
            locale: api::Locale::new(
                api::Language::from_iso(iso_639_1).unwrap(),
                api::Country::from_iso(iso_3166_1).unwrap(),
            ),
            name: name.map(str::to_owned),
            title: None,
            overview: Some(format!("{iso_639_1} overview")),
        };

        let mut draft = ShowDraft {
            original_language: locale("jpn"),
            original_name: Some("ロメリア戦記".to_owned()),
            ..ShowDraft::default()
        };

        let targets = BTreeSet::from([locale("jpn"), locale("eng"), locale("swe"), locale("nld")]);

        let translations = vec![
            translation("ja", "JP", None),
            translation("en", "US", Some("Romelia War Chronicle")),
            translation("nl", "NL", None),
        ];

        add_tmdb_show_strings(&mut draft, translations, &targets);

        let titles: Vec<_> = draft
            .show_strings
            .iter()
            .filter(|(_, kind, _)| *kind == api::StringKind::Title)
            .map(|(locale, _, text)| (locale.to_string(), text.as_str()))
            .collect();

        assert_eq!(
            titles,
            [
                ("ja-JP".to_owned(), "ロメリア戦記"),
                ("en-US".to_owned(), "Romelia War Chronicle"),
            ]
        );
    }

    /// A show left without an enabled Base remote keeps its seasons and
    /// episodes, since deleting them cascades to every user's pending rows.
    #[tokio::test]
    async fn sync_without_base_remote_keeps_seasons() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = Database::open(dir.path().join("test.db"), OpenMode::Normal, 1)?;
        let show_id = api::ShowId::new(1);
        db.create_show(show_id, "Show", None, "").await?;

        let draft = draft(1, "/poster.jpg");
        db.transaction(move |s| write_show_draft(s, show_id, None, &draft))
            .await?;

        let http = reqwest::Client::new();
        let (tx, _) = tokio::sync::broadcast::channel(16);

        sync_show(
            show_id,
            &db,
            &RemoteClients::new(http.clone(), http),
            &Broadcaster::new(tx),
            &crate::pending::PendingSystem::new(db.clone()),
            &Shutdown::new(),
        )
        .await?;

        let seasons: Vec<_> = db
            .seasons(None, show_id)
            .await?
            .into_iter()
            .map(|s| s.season)
            .collect();
        assert_eq!(seasons, [SeasonNumber::from_ordinal(1)]);
        Ok(())
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    fn at(ms: i64) -> api::Timestamp {
        api::Timestamp::from_jiff(jiff::Timestamp::from_millisecond(ms).unwrap())
    }

    const NOW: i64 = 1_700_000_000_000;
    const HOUR: i64 = 3_600_000;

    fn error(key: &str, kind: api::RemoteErrorKind, age_ms: i64) -> api::RemoteError {
        api::RemoteError {
            key: key.to_owned(),
            message: "failed".to_owned(),
            kind,
            at: at(NOW - age_ms),
        }
    }

    fn kinds(kinds: &[SyncKind]) -> SyncKindSet {
        let mut set = SyncKindSet::empty();

        for kind in kinds {
            set.insert(*kind);
        }

        set
    }

    fn cache(etag: &str, covered: &[SyncKind], errors: Vec<api::RemoteError>) -> api::RemoteCache {
        api::RemoteCache {
            etag: Some(etag.to_owned()),
            last_updated: None,
            kinds: kinds(covered),
            errors,
        }
    }

    #[test]
    fn usable_cache_requires_skip_kinds_and_no_expired_errors() {
        let now = at(NOW);
        let base = kinds(&[SyncKind::Base]);

        let covering = cache("a", &[SyncKind::Base, SyncKind::Dates], Vec::new());
        assert!(usable_cache(&covering, base, true, now));
        assert!(!usable_cache(&covering, base, false, now));

        // An air-date-only validator must not short-circuit a Base fetch.
        let dates_only = cache("a", &[SyncKind::Dates], Vec::new());
        assert!(!usable_cache(&dates_only, base, true, now));
        assert!(usable_cache(&dates_only, SyncKindSet::empty(), true, now));

        // A live error keeps short-circuiting; an expired one forces the full fetch.
        let live = cache(
            "a",
            &[SyncKind::Base],
            vec![error("k", api::RemoteErrorKind::Transient, HOUR - 1)],
        );
        assert!(usable_cache(&live, base, true, now));

        let expired = cache(
            "a",
            &[SyncKind::Base],
            vec![error("k", api::RemoteErrorKind::Transient, HOUR)],
        );
        assert!(!usable_cache(&expired, base, true, now));
    }

    #[test]
    fn error_ttl_depends_on_kind() {
        let missing = error("k", api::RemoteErrorKind::Missing, 2 * HOUR);
        let transient = error("k", api::RemoteErrorKind::Transient, 2 * HOUR);

        assert!(missing.is_live(at(NOW)));
        assert!(!transient.is_live(at(NOW)));
    }

    #[test]
    fn needed_kinds_maps_flags() {
        assert_eq!(needed_kinds(false, false, false), SyncKindSet::empty());
        assert_eq!(
            needed_kinds(true, false, true),
            kinds(&[SyncKind::Base, SyncKind::Credits])
        );
        assert_eq!(needed_kinds(false, true, false), kinds(&[SyncKind::Dates]));
    }

    #[test]
    fn cache_json_omits_empty_cache() {
        let empty = cache("", &[], Vec::new());
        let empty = api::RemoteCache {
            etag: None,
            ..empty
        };
        assert_eq!(cache_json(&empty), None);

        let with_etag = cache("abc", &[], Vec::new());
        let json = cache_json(&with_etag).unwrap();
        let back: api::RemoteCache = serde_json::from_str(&json).unwrap();
        assert_eq!(back, with_etag);
    }

    #[test]
    fn suppressed_skips_live_errors_and_keeps_original_timestamp() {
        let live = error("live", api::RemoteErrorKind::Missing, HOUR);
        let stale = error("stale", api::RemoteErrorKind::Transient, 2 * HOUR);
        let prior = cache("a", &[SyncKind::Base], vec![live.clone(), stale]);

        let mut state = CacheState::new(Some(&prior), at(NOW));

        assert!(state.suppressed("live"));
        assert!(!state.suppressed("stale"));
        assert!(!state.suppressed("unknown"));

        // Only the live error is carried forward, with its original timestamp.
        assert_eq!(state.errors, vec![live]);
        assert!(state.degraded());
    }

    #[test]
    fn finish_keeps_validator_only_as_persisted() {
        let prior = cache("old", &[SyncKind::Base], Vec::new());
        let earned = cache("new", &[SyncKind::Base, SyncKind::Credits], Vec::new());

        let mut state = CacheState::new(Some(&prior), at(NOW));
        assert!(!state.degraded());

        // Nothing earned (a 304): the prior validator survives.
        assert_eq!(state.finish(true), prior);

        state.earn(earned.clone());
        assert_eq!(state.finish(true), earned);

        // Not persisted: the earned validator must not claim data that never landed.
        assert_eq!(state.finish(false), prior);
    }

    #[test]
    fn finish_carries_errors_and_defaults_without_validator() {
        let mut state = CacheState::new(None, at(NOW));
        state.record_missing("translations", "none");

        let finished = state.finish(true);
        assert_eq!(finished.etag, None);
        assert_eq!(finished.kinds, SyncKindSet::empty());
        assert_eq!(finished.errors.len(), 1);
        assert_eq!(finished.errors[0].key, "translations");
        assert_eq!(finished.errors[0].kind, api::RemoteErrorKind::Missing);
        assert_eq!(finished.errors[0].at, at(NOW));
        assert!(state.degraded());

        // Errors survive even when the layer's data was not persisted.
        assert_eq!(state.finish(false).errors.len(), 1);
    }

    #[test]
    fn classify_error_treats_non_http_failures_as_transient() {
        let error = anyhow::anyhow!("connection reset");
        assert_eq!(classify_error(&error), api::RemoteErrorKind::Transient);
    }

    #[tokio::test]
    async fn recover_records_failure_and_suppresses_retry() {
        let mut state = CacheState::new(None, at(NOW));

        let value = recover(&mut state, "k", async { Ok::<_, anyhow::Error>(7) }).await;
        assert_eq!(value, Some(7));
        assert!(!state.degraded());

        let failed: Option<i32> =
            recover(&mut state, "k", async { Err(anyhow::anyhow!("boom")) }).await;
        assert_eq!(failed, None);
        assert_eq!(state.errors.len(), 1);
        assert_eq!(state.errors[0].key, "k");

        // The next run sees the recorded failure and does not call at all.
        let prior = state.finish(true);
        let mut next = CacheState::new(Some(&prior), at(NOW + 1000));
        let skipped: Option<i32> =
            recover(&mut next, "k", async { panic!("must not be polled") }).await;
        assert_eq!(skipped, None);
        assert_eq!(next.errors.len(), 1);
    }
}
