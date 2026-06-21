use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Context as _, Result};
use api::{
    EpisodeId, Image, ImageId, ImageKey, ImageKind, RemoteSource, SeasonNumber, SyncKind,
    SyncKindSet,
};
use tracing::{info, warn};

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::remote::RemoteClients;

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
    let base_language = show.language.or(config.language);
    let base_language = base_language.to_iso639_1();

    // Ensure a TVmaze remote is stored (resolved via TVDB/IMDb) so air-date
    // enrichment participates in the layered order, as it did unconditionally
    // before. Best-effort: a failure here just means no TVmaze layer.
    if show.remote_by_source(RemoteSource::Tvmaze).is_none()
        && let Err(e) = ensure_tvmaze_remote(show_id, &show, remote, db).await
    {
        warn!("TVmaze id resolution skipped for show {show_id}: {e:#}");
    }

    // Re-read so a freshly stored TVmaze remote is included in the order.
    let show = db
        .show_by_id(show_id)
        .await?
        .context("Expected show to exist")?;

    info!(show_id = %show_id, title = show.title, ?base_language, "Syncing show");

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
                    tmdb_layer(
                        &mut draft,
                        &show,
                        tmdb_id,
                        base_language,
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
                    tvdb_layer(
                        &mut draft,
                        tvdb_id,
                        base_language,
                        do_base,
                        do_air_date,
                        remote,
                    )
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
            warn!(?source, "Sync layer failed for show {show_id}: {e:#}");
            continue;
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
    //   - eligible, missing   → a configured Base source failed this run, so keep
    //                           the existing show rather than wiping it;
    //   - not eligible        → no enabled remote contributes Base, so the
    //                           seasons/episodes are orphaned and get cleared.
    if draft.provided.contains(SyncKind::Base) {
        // Populate per-language translated strings alongside the direct columns.
        // Best-effort: a failure here must not abort the rest of the sync.
        if let Err(e) = collect_show_strings(&mut draft, &config, remote).await {
            warn!("String collection failed for show {show_id}: {e:#}");
        }

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
    info!(show_id = %show_id, "Sync complete");
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
            info!(tvdb_id = id, "Looking up TVmaze id via TVDB");
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
            info!(imdb_id, "Looking up TVmaze id via IMDB");
            break 'id remote.lookup_tvmaze_by_imdb(imdb_id).await?;
        }

        info!(show_id = %show_id, "Skipping TVmaze id resolution: no TVDB or IMDB remote");
        return Ok(());
    };

    let Some(tvmaze_id) = tvmaze_id else {
        info!(show_id = %show_id, "TVmaze id not found");
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
    air_date: Option<api::Timestamp>,
    name: Option<String>,
    overview: Option<String>,
    poster: Option<Image>,
}

/// An episode's metadata contributed by the base layer.
struct EpisodeDraft {
    absolute_number: Option<u32>,
    name: Option<String>,
    overview: Option<String>,
    aired: Option<api::Timestamp>,
    screenshot: Option<Image>,
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
    title: Option<String>,
    first_air_date: Option<api::Timestamp>,
    overview: Option<String>,
    /// The show's own original language, discovered from the Base layer.
    default_language: Option<api::Language>,
    /// The source/id of the Base provider, used to re-fetch per-language strings.
    base_remote: Option<(RemoteSource, u32)>,
    seasons: BTreeMap<SeasonNumber, SeasonDraft>,
    episodes: BTreeMap<(SeasonNumber, u32), EpisodeDraft>,
    remotes: Vec<(Option<String>, api::Remote)>,
    releases: Vec<DraftRelease>,
    images: Vec<DraftImage>,
    selected: HashMap<ImageKind, ImageKey>,
    /// Per-language translated strings keyed by owner, populated for every target
    /// language in [`collect_show_strings`].
    show_strings: StringRows,
    season_strings: BTreeMap<SeasonNumber, StringRows>,
    episode_strings: BTreeMap<(SeasonNumber, u32), StringRows>,
}

/// A batch of `(language, kind, text)` rows destined for a `*_strings` table.
type StringRows = Vec<(api::Language, api::StringKind, String)>;

/// Append a translated string, skipping missing or blank text.
fn push_string(
    rows: &mut StringRows,
    language: api::Language,
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
        language: api::Language,
        kind: api::StringKind,
        text: Option<String>,
    ) {
        push_string(&mut self.show_strings, language, kind, text);
    }

    fn add_season_string(
        &mut self,
        season: SeasonNumber,
        language: api::Language,
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
        language: api::Language,
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

async fn tmdb_layer(
    draft: &mut ShowDraft,
    show: &api::Show,
    tmdb_id: u32,
    base_language: Option<&str>,
    do_base: bool,
    do_air_date: bool,
    remote: &RemoteClients,
) -> Result<()> {
    info!(tmdb_id, do_base, do_air_date, "Fetching TMDB show");

    let info = remote.fetch_tmdb_show(tmdb_id, base_language).await?;

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

    // When no language is configured, use the show's own original language for
    // episode fetches so episode titles and overviews are also localized.
    let effective_language: Option<&str> =
        base_language.or_else(|| info.original_language.as_deref().filter(|&l| l != "en"));

    if do_base {
        draft.title = info.title.clone();
        draft.overview = info.overview.clone();
        draft.first_air_date = info.first_air_date.or(show.first_air_date);
        draft.default_language = info
            .original_language
            .as_deref()
            .and_then(api::Language::from_iso639);
        draft.base_remote = Some((RemoteSource::Tmdb, tmdb_id));
    }

    for season in &info.seasons {
        if do_base {
            let entry = draft.seasons.entry(season.number).or_default();
            entry.air_date = season.air_date;
            entry.name = season.name.clone();
            entry.overview = season.overview.clone();
            entry.poster = season.poster.clone().map(Image::from);
        }

        info!(tmdb_id, season = ?season.number, "Fetching TMDB season episodes");

        for ep in remote
            .fetch_tmdb_season_episodes(tmdb_id, season.number, effective_language)
            .await?
        {
            if do_base {
                draft.episodes.insert(
                    (ep.season, ep.number),
                    EpisodeDraft {
                        absolute_number: None,
                        name: ep.name,
                        overview: ep.overview,
                        aired: ep.aired,
                        screenshot: ep.filename.map(Image::from),
                    },
                );
            }

            if do_air_date && let Some(aired) = ep.aired {
                draft.releases.push(DraftRelease {
                    season: ep.season,
                    number: ep.number,
                    source: RemoteSource::Tmdb,
                    country: api::Country::DEFAULT,
                    network: String::new(),
                    timestamp: aired,
                });
            }
        }
    }

    Ok(())
}

async fn tvdb_layer(
    draft: &mut ShowDraft,
    tvdb_id: u32,
    base_language: Option<&str>,
    do_base: bool,
    do_air_date: bool,
    remote: &RemoteClients,
) -> Result<()> {
    info!(tvdb_id, do_base, do_air_date, "Fetching TVDB show");

    let info = remote.fetch_tvdb_show(tvdb_id, base_language).await?;

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

    if !do_base && !do_air_date {
        return Ok(());
    }

    if do_base {
        draft.title = info.title.clone();
        draft.overview = info.overview.clone();
        // TVDB has no first-air-date field; persist falls back to the existing value.
        draft.default_language = info
            .original_language
            .as_deref()
            .and_then(api::Language::from_iso639);
        draft.base_remote = Some((RemoteSource::Tvdb, tvdb_id));
    }

    // When no language is configured, use the show's own original language for
    // episode fetches. TVDB uses 3-letter language codes; "eng" is the default.
    let effective_language: Option<&str> =
        base_language.or_else(|| info.original_language.as_deref().filter(|&l| l != "eng"));

    info!(tvdb_id, "Fetching TVDB episodes");
    let episodes = remote
        .fetch_tvdb_episodes(tvdb_id, effective_language)
        .await?;
    info!(count = episodes.len(), "Got episodes from TVDB");

    for ep in episodes {
        if do_base {
            // TVDB has no season records; derive the air date as the earliest
            // episode air date in the season.
            let entry = draft.seasons.entry(ep.season).or_default();

            if let Some(aired) = ep.aired {
                entry.air_date = Some(match entry.air_date {
                    Some(cur) if cur <= aired => cur,
                    _ => aired,
                });
            }

            let screenshot = ep
                .image
                .as_ref()
                .map(|(source, path)| Image::new(*source, path));

            draft.episodes.insert(
                (ep.season, ep.number),
                EpisodeDraft {
                    absolute_number: ep.absolute_number,
                    name: ep.name,
                    overview: ep.overview,
                    aired: ep.aired,
                    screenshot,
                },
            );
        }

        if do_air_date && let Some(aired) = ep.aired {
            draft.releases.push(DraftRelease {
                season: ep.season,
                number: ep.number,
                source: RemoteSource::Tvdb,
                country: api::Country::DEFAULT,
                network: String::new(),
                timestamp: aired,
            });
        }
    }

    Ok(())
}

#[tracing::instrument(skip_all, fields(show_id = %show_id))]
async fn tvmaze_layer(
    draft: &mut ShowDraft,
    show_id: api::ShowId,
    tvmaze_id: u32,
    remote: &RemoteClients,
) -> Result<()> {
    let network = match remote.fetch_tvmaze_show_network(tvmaze_id).await {
        Ok(network) => network,
        Err(e) => {
            warn!("TVmaze network lookup failed for show {show_id}: {e:#}");
            Default::default()
        }
    };

    info!(tvmaze_id, "Fetching TVmaze episodes");

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

    info!(episodes = count, "Collected TVmaze air dates");

    Ok(())
}

/// Fetch and record per-language translated strings for the show, its seasons
/// and its episodes. The set of languages is [`api::expand_sync_languages`] of
/// the configured `sync_languages` against the show's own original language; the
/// strings come from the same source that provided the Base layer.
async fn collect_show_strings(
    draft: &mut ShowDraft,
    config: &api::Config,
    remote: &RemoteClients,
) -> Result<()> {
    let Some((source, id)) = draft.base_remote else {
        return Ok(());
    };

    let original = draft.default_language.unwrap_or(api::Language::DEFAULT);
    let targets = api::expand_sync_languages(&config.sync_languages, original);

    for language in targets {
        // Remotes key on ISO 639-1; skip any language without a 2-letter form.
        let Some(iso) = language.to_iso639_1() else {
            continue;
        };

        match source {
            RemoteSource::Tmdb => collect_tmdb_strings(draft, id, language, iso, remote).await?,
            RemoteSource::Tvdb => collect_tvdb_strings(draft, id, language, iso, remote).await?,
            _ => {}
        }
    }

    Ok(())
}

async fn collect_tmdb_strings(
    draft: &mut ShowDraft,
    tmdb_id: u32,
    language: api::Language,
    iso: &str,
    remote: &RemoteClients,
) -> Result<()> {
    let info = remote.fetch_tmdb_show(tmdb_id, Some(iso)).await?;

    draft.add_show_string(language, api::StringKind::Title, info.title.clone());
    draft.add_show_string(language, api::StringKind::Overview, info.overview.clone());

    let seasons: Vec<SeasonNumber> = info
        .seasons
        .iter()
        .map(|season| {
            draft.add_season_string(
                season.number,
                language,
                api::StringKind::Title,
                season.name.clone(),
            );
            draft.add_season_string(
                season.number,
                language,
                api::StringKind::Overview,
                season.overview.clone(),
            );
            season.number
        })
        .collect();

    for season in seasons {
        for ep in remote
            .fetch_tmdb_season_episodes(tmdb_id, season, Some(iso))
            .await?
        {
            draft.add_episode_string(
                ep.season,
                ep.number,
                language,
                api::StringKind::Title,
                ep.name,
            );
            draft.add_episode_string(
                ep.season,
                ep.number,
                language,
                api::StringKind::Overview,
                ep.overview,
            );
        }
    }

    Ok(())
}

async fn collect_tvdb_strings(
    draft: &mut ShowDraft,
    tvdb_id: u32,
    language: api::Language,
    iso: &str,
    remote: &RemoteClients,
) -> Result<()> {
    let info = remote.fetch_tvdb_show(tvdb_id, Some(iso)).await?;

    draft.add_show_string(language, api::StringKind::Title, info.title.clone());
    draft.add_show_string(language, api::StringKind::Overview, info.overview.clone());

    // TVDB has no season records, so only show- and episode-level strings.
    for ep in remote.fetch_tvdb_episodes(tvdb_id, Some(iso)).await? {
        draft.add_episode_string(
            ep.season,
            ep.number,
            language,
            api::StringKind::Title,
            ep.name,
        );
        draft.add_episode_string(
            ep.season,
            ep.number,
            language,
            api::StringKind::Overview,
            ep.overview,
        );
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
        draft.title.as_deref(),
        draft.first_air_date.or(show.first_air_date),
        draft.overview.as_deref(),
        show.tracked,
    )
    .await?;

    if let Some(language) = draft.default_language {
        db.set_show_default_language(show_id, language).await?;
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
            ep.name.as_deref(),
            ep.overview.as_deref(),
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
        let season_id = db
            .upsert_season(
                show_id,
                *number,
                season.air_date,
                season.name.as_deref(),
                season.overview.as_deref(),
            )
            .await?;

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
    // Track what we wrote so stale releases (and only for sources that synced this
    // run) can be pruned afterwards.
    let mut kept_releases = HashSet::new();
    let mut release_sources = HashSet::new();

    for r in &draft.releases {
        let Some(&episode_id) = episode_ids.get(&(r.season, r.number)) else {
            continue;
        };

        db.upsert_episode_release(episode_id, r.source, r.country, &r.network, r.timestamp)
            .await?;

        kept_releases.insert((episode_id, r.source, r.country, r.network.clone()));
        release_sources.insert(r.source);
    }

    db.prune_episode_releases(show_id, &kept_releases, &release_sources)
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
    let language = language.to_iso639_1();

    let source = movie.primary_sync_source();
    info!(movie_id = %movie_id, title = movie.title, ?source, ?language, "Syncing movie");

    match source {
        Some(api::RemoteSource::Tmdb) => {
            let remote_id = movie
                .remote_by_source(api::RemoteSource::Tmdb)
                .context("Expected movie to have a TMDB remote")?;

            let tmdb_id: u32 = remote_id
                .value()
                .as_u32()
                .context("Expected a valid TMDB id")?;
            info!(tmdb_id, "Fetching TMDB movie");

            let info = remote.fetch_tmdb_movie(tmdb_id, language).await?;

            db.update_movie(
                movie_id,
                info.title.as_deref(),
                info.release_date.or(movie.release_date),
                info.overview.as_deref(),
            )
            .await?;

            // Per-language translated strings alongside the direct columns.
            let original = info
                .original_language
                .as_deref()
                .and_then(api::Language::from_iso639);

            if let Some(original) = original {
                db.set_movie_default_language(movie_id, original).await?;
            }

            if let Err(e) =
                collect_movie_strings(movie_id, tmdb_id, original, &config, db, remote).await
            {
                warn!(movie_id = %movie_id, "String collection failed: {e:#}");
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
                    info!(count = releases.len(), "Fetched TMDB movie releases");

                    let mut movie_releases = db.movie_releases(movie_id).await?;

                    for r in releases {
                        movie_releases.retain(|mr| {
                            !(mr.country == r.country && mr.release_type == r.release_type)
                        });

                        db.upsert_movie_release(
                            movie_id,
                            r.country,
                            r.release_type,
                            &r.release_date,
                        )
                        .await?;
                    }

                    for r in movie_releases {
                        db.delete_movie_release(movie_id, r.country, r.release_type)
                            .await?;
                    }
                }
                Err(e) => warn!(movie_id = %movie_id, "Movie release dates skipped: {e:#}"),
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
    info!(movie_id = %movie_id, "Sync complete");
    Ok(())
}

/// Fetch and replace a movie's per-language translated strings. Languages are
/// [`api::expand_sync_languages`] of the configured `sync_languages` against the
/// movie's own original language.
async fn collect_movie_strings(
    movie_id: api::MovieId,
    tmdb_id: u32,
    original: Option<api::Language>,
    config: &api::Config,
    db: &Database,
    remote: &RemoteClients,
) -> Result<()> {
    let targets = api::expand_sync_languages(
        &config.sync_languages,
        original.unwrap_or(api::Language::DEFAULT),
    );

    let mut rows: StringRows = Vec::new();

    for language in targets {
        let Some(iso) = language.to_iso639_1() else {
            continue;
        };

        let info = remote.fetch_tmdb_movie(tmdb_id, Some(iso)).await?;
        push_string(&mut rows, language, api::StringKind::Title, info.title);
        push_string(
            &mut rows,
            language,
            api::StringKind::Overview,
            info.overview,
        );
    }

    db.replace_movie_strings(movie_id, rows).await?;
    Ok(())
}
