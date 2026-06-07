# Plan: TVMaze enrichment for exact air datetimes

## 0. Background & goal

The app currently stores episode air dates with **calendar-day precision only**:
`Episode.aired: Option<Date>` where `Date` is a `CivilDate` stored in SQLite as an
`INTEGER` in `YYYYMMDD` form (e.g. `20240115`).

TVMaze exposes `airstamp` — an ISO 8601 datetime **with timezone**, e.g.
`"2019-09-24T22:00:00+00:00"` — which gives the exact broadcast instant. We want to
fetch and store this alongside the existing day-precision field, and surface it in
the episode list UI.

This is a **greenfield** project: no production users, no live DB. The single
migration file may be rewritten wholesale. No backwards compatibility is required.

---

## 1. Decision summary — Approach B (TVMaze as a supplementary enrichment step)

TVMaze runs as a **best-effort side effect of every series sync**, after the primary
TMDB or TVDB sync completes. It does not replace or compete with the primary source.

### Why B over A (TVMaze as a third SyncSource)

Approach A requires a series to have TVMaze as its *primary* sync source to populate
`aired_at`. A series synced from TMDB would always have `aired_at = NULL`, because
TVMaze only runs when it owns the sync. To get exact airtimes for an existing
TMDB/TVDB series, the user would have to switch its sync source to TVMaze entirely,
losing TMDB's season posters, backdrop images, and any TMDB-specific metadata. That
means you effectively need two SyncSources per entity — which the data model does not
support. The `aired_at` column would be useful only for the small subset of series
explicitly switched to TVMaze.

Approach B populates `aired_at` for **every** series regardless of its primary sync
source. TMDB and TVDB remain unchanged. TVMaze is a transparent enrichment layer
that fires after the primary sync, looks up the show via a cross-reference endpoint
(`/lookup/shows?thetvdb={id}` or `?imdb={id}`), and fills in exact airtimes.
If the lookup returns 404 (TVMaze has no entry for the show), the sync still
completes normally.

### The (season, number) matching concern, addressed

The original plan cited cross-reference fragility (404s, numbering mismatches).
In practice: TVMaze uses aired-order `(season, number)`, the same key the existing
`UNIQUE(series_id, season, number)` index is built on. The enrichment step only
writes `aired_at` — it does not create or rename episodes — so a TVMaze episode
with no matching row in the DB is silently skipped, and no harm is done.

### What this approach does NOT do

- No `SyncSource::Tvmaze` variant — `SyncSource` stays `{ Tmdb, Tvdb }`.
- No `ImageSource::Tvmaze` — TVMaze images are skipped (absolute URLs require proxy
  changes; air datetimes are the goal).
- No Config/settings change — TVMaze needs no API key.
- No change to `upsert_episode`'s signature — TMDB/TVDB callers are untouched.

---

## 2. TVMaze API reference

Base URL: `https://api.tvmaze.com`. No API key required. Rate limit ~20 req/s.

| Endpoint | Purpose |
|---|---|
| `GET /lookup/shows?thetvdb={id}` | Resolve TVDB series id → TVMaze show id (or 404) |
| `GET /lookup/shows?imdb={id}` | Resolve IMDB series id → TVMaze show id (or 404) |
| `GET /shows/{id}/episodes` | All episodes for a TVMaze show (single request, no pagination) |

### Episode object (fields we use)

```jsonc
{
  "id": 1,
  "season": 1,
  "number": 1,                              // null for some specials
  "airdate": "2013-06-24",                  // "YYYY-MM-DD" or null/""
  "airstamp": "2013-06-24T22:00:00+00:00",  // ISO 8601 w/ tz offset, or null
}
```

`airstamp` is what we store as `aired_at`. `jiff::Timestamp::from_str` parses RFC
3339 with offset and normalises to UTC. `Timestamp` already has `FromStr` delegating
to `jiff::Timestamp`, and `sqll` `BindValue` stores it as epoch ms `INTEGER` — no
new conversion code beyond a one-line `opt_timestamp` helper.

### Lookup response

`GET /lookup/shows?thetvdb=264492` returns a show JSON object (HTTP 200) or 404.
We only need the `id` field from the show object.

---

## 3. Schema changes (`crates/db/migrations/2026-06-05.sql`)

Add `aired_at INTEGER` to the `episodes` table. Rewrite the `CREATE TABLE`
statement (greenfield, do not stack ALTER TABLE):

```sql
CREATE TABLE episodes (
    id              INTEGER PRIMARY KEY,
    series_id       INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    season          INTEGER NOT NULL,
    number          INTEGER NOT NULL,
    absolute_number INTEGER,
    name            TEXT,
    overview        TEXT NOT NULL DEFAULT '',
    aired           INTEGER,        -- YYYYMMDD (day precision; used by pending/schedule queries)
    aired_at        INTEGER,        -- epoch ms (exact broadcast instant from TVMaze); nullable
    filename        TEXT,
    remote_id       TEXT,
    UNIQUE(series_id, season, number)
);

CREATE INDEX idx_episodes_aired    ON episodes (aired)    WHERE aired    IS NOT NULL;
CREATE INDEX idx_episodes_aired_at ON episodes (aired_at) WHERE aired_at IS NOT NULL;
```

Delete the local dev DB after the rewrite so the migration applies cleanly.

No other tables change.

---

## 4. `crates/api` changes (`crates/api/src/lib.rs`)

### 4.1 `Episode` — add `aired_at`

```rust
pub struct Episode {
    pub id: EpisodeId,
    pub series_id: SeriesId,
    pub season: SeasonNumber,
    pub number: u32,
    pub absolute_number: Option<u32>,
    pub name: Option<String>,
    pub overview: String,
    pub aired: Option<Date>,
    pub aired_at: Option<Timestamp>,   // NEW: exact broadcast instant (TVMaze), epoch ms
    pub filename: Option<Image>,
    pub remote_id: Option<RemoteId>,
    pub watched: bool,
    pub watched_count: u32,
    pub last_watched_id: Option<WatchedId>,
}
```

`Timestamp` already implements `Encode`/`Decode` (as RFC 3339 string on the wire)
and `sqll` `FromColumn`/`BindValue` (as epoch ms `INTEGER`). No new impls needed.

### 4.2 Nothing else changes in `api`

`SyncSource`, `ImageSource`, `RemoteId`, `Config`, `Pending`, `ScheduledEntry` —
all unchanged.

---

## 5. `crates/db` changes (`crates/db/src/lib.rs`)

### 5.1 `EpisodeRow` — add `aired_at`

```rust
#[derive(Row)]
struct EpisodeRow {
    id: EpisodeId,
    series_id: SeriesId,
    season: i64,
    number: i64,
    absolute_number: Option<i64>,
    name: Option<String>,
    overview: String,
    aired: Option<Date>,
    aired_at: Option<Timestamp>,   // NEW
    filename: Option<Image>,
    remote_id: Option<RemoteId>,
    watched: bool,
    watched_count: i64,
    last_watched_id: Option<WatchedId>,
}
```

### 5.2 `episode_from_row` — map the new field

```rust
fn episode_from_row(r: EpisodeRow) -> api::Episode {
    api::Episode {
        // …unchanged fields…
        aired: r.aired,
        aired_at: r.aired_at,   // NEW
        // …
    }
}
```

### 5.3 `upsert_episode` SQL — add `aired_at` to `RETURNING` only

`upsert_episode` does not write `aired_at`. TMDB/TVDB callers remain unchanged
(no new parameter), and the column's value is preserved across re-syncs (the
`ON CONFLICT DO UPDATE SET` does not include it, so SQLite leaves it alone).
Add `aired_at` only to the `RETURNING` clause so the returned `EpisodeRow` has
the field:

```sql
upsert_episode: r#"
    INSERT INTO episodes (series_id, season, number, absolute_number, name, overview, aired, filename, remote_id)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(series_id, season, number) DO UPDATE SET
        absolute_number = excluded.absolute_number,
        name            = excluded.name,
        overview        = excluded.overview,
        aired           = excluded.aired,
        filename        = excluded.filename,
        remote_id       = excluded.remote_id
    RETURNING id, series_id, season, number, absolute_number, name, overview,
              aired, aired_at, filename, remote_id,
              0 AS watched, 0 AS watched_count, NULL AS last_watched_id
"#,
```

### 5.4 `list_episodes` and `episode_by_id` SELECTs — add `e.aired_at`

Both statements currently select `e.aired, e.filename, e.remote_id`. Change to
`e.aired, e.aired_at, e.filename, e.remote_id` so the shape matches `EpisodeRow`.

### 5.5 New statement: `update_episode_aired_at`

```sql
update_episode_aired_at: r#"
    UPDATE episodes SET aired_at = ?
    WHERE series_id = ? AND season = ? AND number = ?
"#,
```

### 5.6 New method: `update_episodes_aired_at`

Batch-updates `aired_at` for all TVMaze-matched episodes in one `spawn_blocking`
call, wrapped in a transaction for efficiency:

```rust
pub async fn update_episodes_aired_at(
    &self,
    series_id: SeriesId,
    updates: Vec<(SeasonNumber, u32, Timestamp)>,
) -> Result<()> {
    if updates.is_empty() { return Ok(()); }
    self.inner.call(move |conn| {
        let s = conn.stmts();
        for (season, number, aired_at) in &updates {
            s.update_episode_aired_at.bind((
                aired_at,
                series_id,
                season.to_i64(),
                *number as i64,
            ))?;
            s.update_episode_aired_at.step_done()?;
        }
        Ok(())
    }).await
}
```

---

## 6. New file: `crates/server/src/tvmaze.rs`

No auth, no pagination. Register with `mod tvmaze;` in `main.rs`.

```rust
use anyhow::Result;
use api::{SeasonNumber, Timestamp};
use serde::Deserialize;

const BASE: &str = "https://api.tvmaze.com";

#[derive(Clone)]
pub(crate) struct Client {
    http: reqwest::Client,
}

impl Client {
    pub(crate) fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    /// Returns the TVMaze show id for the given TVDB series id, or None if not found.
    pub(crate) async fn lookup_by_tvdb(&self, tvdb_id: u32) -> Result<Option<u32>> {
        self.lookup("thetvdb", tvdb_id).await
    }

    /// Returns the TVMaze show id for the given IMDB series id, or None if not found.
    pub(crate) async fn lookup_by_imdb(&self, imdb_id: &str) -> Result<Option<u32>> {
        self.lookup("imdb", imdb_id).await
    }

    async fn lookup(&self, param: &str, id: u32) -> Result<Option<u32>> {
        #[derive(Deserialize)]
        struct Show { id: u32 }

        let resp = self.http
            .get(format!("{BASE}/lookup/shows"))
            .query(&[(param, id.to_string())])
            .send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let bytes = resp.error_for_status()?.bytes().await?;
        let show: Show = serde_json::from_slice(&bytes)?;
        Ok(Some(show.id))
    }

    /// Returns all episodes for a TVMaze show with their exact airstamps.
    pub(crate) async fn fetch_episodes(&self, id: u32) -> Result<Vec<EpisodeInfo>> {
        #[derive(Deserialize)]
        struct Row {
            #[serde(default)]
            season: Option<u32>,
            #[serde(default)]
            number: Option<u32>,
            #[serde(default)]
            airstamp: Option<String>,
        }

        let bytes = self.http
            .get(format!("{BASE}/shows/{id}/episodes"))
            .send().await?.error_for_status()?.bytes().await?;
        let rows: Vec<Row> = serde_json::from_slice(&bytes)?;

        Ok(rows.into_iter().filter_map(|r| {
            let aired_at = r.airstamp.as_deref()
                .filter(|s| !s.is_empty())
                .and_then(|s| s.parse::<Timestamp>().ok())?;  // skip if no parseable airstamp
            Some(EpisodeInfo {
                season: match r.season {
                    Some(n) if n > 0 => SeasonNumber::Number(n),
                    _ => SeasonNumber::Specials,
                },
                number: r.number.unwrap_or(0),
                aired_at,
            })
        }).collect())
    }
}

pub(crate) struct EpisodeInfo {
    pub season: SeasonNumber,
    pub number: u32,
    pub aired_at: Timestamp,
}
```

Note: episodes with no parseable `airstamp` are filtered out — they have nothing to
contribute to the enrichment step.

---

## 7. `crates/server/src/remote.rs` — add the TVMaze client

TVMaze needs no key, so its client is **always present** (constructed in `new()`).
Keep it as `Option` for structural uniformity with tmdb/tvdb, but always initialise
it.

```rust
#[derive(Default)]
struct Inner {
    tmdb: Option<crate::tmdb::Client>,
    tvdb: Option<crate::tvdb::Client>,
    tvmaze: Option<crate::tvmaze::Client>,   // always Some after new()
}

impl RemoteClients {
    pub(crate) fn new(http: reqwest::Client) -> Self {
        let inner = Inner {
            tvmaze: Some(crate::tvmaze::Client::new(http.clone())),
            ..Default::default()
        };
        Self { http, inner: Arc::new(Mutex::new(inner)) }
    }

    pub(crate) fn configure(&self, config: &api::Config) {
        let mut inner = self.inner.lock();
        inner.tmdb = /* unchanged */;
        inner.tvdb = /* unchanged */;
        // Ensure the keyless TVMaze client is always present:
        if inner.tvmaze.is_none() {
            inner.tvmaze = Some(crate::tvmaze::Client::new(self.http.clone()));
        }
    }

    fn tvmaze(&self) -> Option<crate::tvmaze::Client> {
        self.inner.lock().tvmaze.clone()
    }

    pub(crate) async fn lookup_tvmaze_by_tvdb(&self, id: u32) -> Result<Option<u32>> {
        self.tvmaze().context("no TVMaze client")?.lookup_by_tvdb(id).await
    }

    pub(crate) async fn lookup_tvmaze_by_imdb(&self, id: u32) -> Result<Option<u32>> {
        self.tvmaze().context("no TVMaze client")?.lookup_by_imdb(id).await
    }

    pub(crate) async fn fetch_tvmaze_episodes(&self, id: u32)
        -> Result<Vec<crate::tvmaze::EpisodeInfo>>
    {
        self.tvmaze().context("no TVMaze client")?.fetch_episodes(id).await
    }
}
```

---

## 8. `crates/server/src/sync.rs` — enrichment step

### 8.1 New `enrich_with_tvmaze` function

Called after the primary sync completes. Errors are non-fatal (logged as warnings)
so a TVMaze outage never breaks a TMDB/TVDB sync.

```rust
async fn enrich_with_tvmaze(
    series_id: api::SeriesId,
    series: &api::Series,
    remote: &RemoteClients,
    db: &Database,
    broadcast: &Broadcaster,
) -> Result<()> {
    // Resolve TVMaze show id via whichever remote the series has.
    let tvmaze_id = if let Some(r) = series.remote_by_source("tvdb") {
        let id: u32 = r.value().parse().context("invalid tvdb id")?;
        remote.lookup_tvmaze_by_tvdb(id).await?
    } else if let Some(r) = series.remote_by_source("imdb") {
        remote.lookup_tvmaze_by_imdb(r.value()).await?
    } else {
        return Ok(());  // no cross-reference possible
    };

    let Some(tvmaze_id) = tvmaze_id else {
        return Ok(());  // TVMaze has no entry for this show — silently skip
    };

    let tvmaze_eps = remote.fetch_tvmaze_episodes(tvmaze_id).await?;

    // Collect seasons that receive an update so we can broadcast EpisodesChanged.
    let mut seasons_updated: HashSet<api::SeasonNumber> = HashSet::new();
    let updates: Vec<(api::SeasonNumber, u32, api::Timestamp)> = tvmaze_eps
        .into_iter()
        .map(|ep| {
            seasons_updated.insert(ep.season);
            (ep.season, ep.number, ep.aired_at)
        })
        .collect();

    db.update_episodes_aired_at(series_id, updates).await?;

    // Notify the frontend that episode data changed.
    for season in seasons_updated {
        broadcast_event(broadcast, api::AppEventKind::EpisodesChanged { series_id, season });
    }

    Ok(())
}
```

### 8.2 Call it from `sync_series`

Add the enrichment call after the primary sync dispatch, before the final
`PendingChanged` broadcast:

```rust
pub(crate) async fn sync_series(
    series_id: api::SeriesId,
    db: &Database,
    remote: &RemoteClients,
    broadcast: &Broadcaster,
) -> Result<()> {
    let series = db.series_by_id(series_id).await?.context("series not found")?;
    let source = series.effective_sync_source();

    match source {
        Some(api::SyncSource::Tmdb) => { /* unchanged */ }
        Some(api::SyncSource::Tvdb) => { /* unchanged */ }
        None => anyhow::bail!("series has no syncable remote (tmdb or tvdb)"),
    }

    // Best-effort TVMaze enrichment for exact airtimes.
    // Re-fetch series after primary sync in case remotes changed.
    if let Some(series) = db.series_by_id(series_id).await? {
        if let Err(e) = enrich_with_tvmaze(series_id, &series, remote, db, broadcast).await {
            tracing::warn!("TVMaze enrichment skipped for series {series_id}: {e:#}");
        }
    }

    broadcast_event(broadcast, api::AppEventKind::PendingChanged);
    Ok(())
}
```

`sync_movie` is unchanged (TVMaze has no movie enrichment).

---

## 9. Frontend — display `aired_at` (`crates/frontend/src/series_detail.rs`)

`Episode.aired_at: Option<Timestamp>` is now available. Where the episode list
renders the air date (currently a `text-muted` span containing `date.to_string()`),
prefer the exact datetime when present:

```rust
if let Some(at) = ep.aired_at {
    <span class="text-muted">{at.to_string()}</span>
} else if let Some(date) = ep.aired {
    <span class="text-muted">{date.to_string()}</span>
}
```

`Timestamp`'s `Display` renders as RFC 3339 UTC, e.g. `"2013-06-24T22:00:00Z"`.
A nicer local-time format is a follow-up; raw RFC 3339 is acceptable for the first
cut. Reuse the existing `text-muted` class — **no new CSS**.

---

## 10. Order of work

1. **`crates/api/src/lib.rs`**: add `aired_at: Option<Timestamp>` to `Episode`.
2. **Migration** (`crates/db/migrations/2026-06-05.sql`): add `aired_at INTEGER`
   column and index to `episodes`. Delete local dev DB.
3. **`crates/db/src/lib.rs`**:
   - Add `aired_at: Option<Timestamp>` to `EpisodeRow`.
   - Map it in `episode_from_row`.
   - Add `aired_at` to `RETURNING` in `upsert_episode` SQL (not in INSERT or UPDATE SET).
   - Add `e.aired_at` to `list_episodes` and `episode_by_id` SELECTs.
   - Add `update_episode_aired_at` statement and `update_episodes_aired_at` method.
4. **`crates/server/src/tvmaze.rs`**: new file per §6; `mod tvmaze;` in `main.rs`.
5. **`crates/server/src/remote.rs`**: add keyless TVMaze client and lookup/fetch helpers.
6. **`crates/server/src/sync.rs`**: add `enrich_with_tvmaze`; call it from `sync_series`.
7. **`cargo build`** (workspace + wasm frontend): fix any compile errors.
8. **`crates/frontend/src/series_detail.rs`**: render `aired_at` with `text-muted`.
9. **Manual test**: sync a TVDB-sourced series; confirm `episodes.aired_at` is
   populated in SQLite and the episode list shows exact times.

---

## 11. Validation checklist

- [ ] `episodes.aired_at` column present in the schema.
- [ ] After syncing a TVDB series, `aired_at` is populated for episodes that have
      TVMaze airstamps.
- [ ] After syncing a TMDB series, same.
- [ ] A series with no TVMaze entry (lookup returns 404) syncs cleanly; warning
      logged, no error returned.
- [ ] `upsert_episode` callers (TMDB/TVDB sync) compile unchanged — no new parameter.
- [ ] Re-syncing a series does not clobber `aired_at` values already written.
- [ ] Episode list in `series_detail` shows exact time for TVMaze-enriched episodes,
      date only for others.
- [ ] No new CSS classes introduced.
