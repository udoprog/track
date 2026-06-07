# Pending table refactor

Replace the two ad-hoc pending mechanisms — `series.pending_episode_id` (a
denormalized "which episode is next" pointer) and `movies.pending` (a manual
boolean override) — with a single dedicated `pending` table modeled exactly like
`watched`: one row per pending item, an `episode_id`/`movie_id` pair with a
CHECK that exactly one is non-null, and a `timestamp` column that **alone**
determines sort order.

**Timestamp semantics (the core rule):**
- **Auto (episode discovered by sync):** `timestamp` = the episode's `aired`
  date, converted to a `Timestamp` (millisecond epoch at UTC midnight of that
  day). Older airings sort to the bottom, newest to the top.
- **Manual (user marks an episode or movie pending):** `timestamp` = `now()`
  (`Timestamp::now()`), so a manually pinned item floats to the top regardless of
  air/release date.
- **Auto (movie past its release date):** `timestamp` = the movie's
  `release_date`, same Date→Timestamp conversion as episodes.

Note the column is INTEGER ms (like `watched.timestamp`), but `aired` /
`release_date` are `Date` (TEXT "YYYY-MM-DD"). The conversion happens in Rust at
insert time, not in SQL — add a helper `Date::to_timestamp(self) -> Timestamp`
in `crates/api` (midnight UTC of the day → `JiffTimestamp` → ms) and use it
wherever an auto pending row is inserted. Do **not** store the Date as TEXT in
this column; keep the column type consistent with `watched`.

## a. Schema — full rewrite of `crates/db/migrations/2026-06-05.sql`

This is greenfield: rewrite the file wholesale, do not stack `ALTER TABLE`.
Changes versus the current file:

- `series`: **drop** the `pending_episode_id` column (and its
  `REFERENCES episodes(...) ON DELETE SET NULL`).
- `movies`: **drop** the `pending` column.
- Add the new table + indexes (place after `watched`, which it mirrors):

```sql
CREATE TABLE pending (
    id         INTEGER PRIMARY KEY,
    timestamp  INTEGER NOT NULL,
    episode_id INTEGER REFERENCES episodes(id) ON DELETE CASCADE,
    movie_id   INTEGER REFERENCES movies(id) ON DELETE CASCADE,
    CHECK((episode_id IS NULL) != (movie_id IS NULL))
);

CREATE INDEX idx_pending_timestamp ON pending (timestamp);
CREATE UNIQUE INDEX idx_pending_episode ON pending (episode_id) WHERE episode_id IS NOT NULL;
CREATE UNIQUE INDEX idx_pending_movie   ON pending (movie_id)   WHERE movie_id   IS NOT NULL;
```

The per-entity indexes are **UNIQUE** (unlike the analogous `remotes`/`images`
ones, and stricter than the brief's draft): an episode or movie can be pending at
most once, which makes the sync upsert logic (`INSERT … ON CONFLICT … DO UPDATE`)
and "is this already pending?" checks trivial and prevents duplicate cards on the
dashboard. `ON DELETE CASCADE` means deleting a series/episode/movie removes its
pending rows for free — so `RemoveSeries`/`RemoveMovie` need no extra cleanup.

## b. DB layer — `crates/db/src/lib.rs`

**Row types:**
- `SeriesRow`: remove `pending_episode_id` field.
- `MovieRow`: remove `pending` field.
- Remove `PendingEpisodeRow` and `PendingMovieRow`; replace with one unified
  `PendingRow` that the new joined query fills:

```rust
#[derive(Row)]
struct PendingRow {
    id: PendingId,
    timestamp: Timestamp,
    episode_id: Option<EpisodeId>,
    movie_id: Option<MovieId>,
    // episode joins (NULL for movie rows)
    series_id: Option<SeriesId>,
    series_title: Option<String>,
    episode_name: Option<String>,
    season: Option<i64>,
    number: Option<i64>,
    // movie joins (NULL for episode rows)
    movie_title: Option<String>,
    // shared
    poster: Option<Image>,
    aired: Option<Date>,   // episode.aired or movie.release_date, for display
}
```

(Introduce a `PendingId` newtype via `define_id!(PendingId)` in `crates/api`.)

**Statements — remove:** `set_series_next_episode`, `set_movie_pending`, and
both `list_pending_episodes` / `list_pending_movies`. Remove the
`pending_episode_id` references from every `series` SELECT (`insert_series`,
`list_series`, `series_by_id`, `series_by_remote`) and the `pending` references
from every `movies` SELECT (`insert_movie`, `list_movies`, `movie_by_id`,
`movie_by_remote`).

**Statements — add:**

```rust
// Insert-or-update a pending row keyed by the unique partial index.
upsert_pending_episode: r#"
    INSERT INTO pending (timestamp, episode_id) VALUES (?, ?)
    ON CONFLICT(episode_id) WHERE episode_id IS NOT NULL
        DO UPDATE SET timestamp = excluded.timestamp
"#,
upsert_pending_movie: r#"
    INSERT INTO pending (timestamp, movie_id) VALUES (?, ?)
    ON CONFLICT(movie_id) WHERE movie_id IS NOT NULL
        DO UPDATE SET timestamp = excluded.timestamp
"#,
delete_pending_episode: r#"DELETE FROM pending WHERE episode_id = ?"#,
delete_pending_movie:   r#"DELETE FROM pending WHERE movie_id = ?"#,
// Check if this series already has a pending episode row. If it does, sync
// leaves it alone (manual/prior pins are never overwritten); the auto-fill
// only fires into an empty slot.
has_pending_episode_for_series: r#"
    SELECT 1 FROM pending p
    JOIN episodes e ON e.id = p.episode_id
    WHERE e.series_id = ?
    LIMIT 1
"#,
// Find the single episode that should be pending for a series:
// the oldest aired, unwatched episode (aired <= today).
next_pending_episode_for_series: r#"
    SELECT e.id, e.aired
    FROM episodes e
    WHERE e.series_id = ?
      AND e.aired IS NOT NULL
      AND e.aired <= ?
      AND NOT EXISTS (SELECT 1 FROM watched w WHERE w.episode_id = e.id)
    ORDER BY e.season, e.number
    LIMIT 1
"#,
// Movies past release, unwatched, not yet in pending.
movies_needing_pending: r#"
    SELECT m.id, m.release_date
    FROM movies m
    WHERE m.tracked = 1
      AND m.release_date IS NOT NULL
      AND m.release_date <= ?
      AND NOT EXISTS (SELECT 1 FROM watched  w WHERE w.movie_id = m.id)
      AND NOT EXISTS (SELECT 1 FROM pending  p WHERE p.movie_id = m.id)
"#,
// The unified list query (replaces both old list_pending_* queries).
list_pending: r#"
    SELECT p.id, p.timestamp, p.episode_id, p.movie_id,
           e.series_id, s.title AS series_title, e.name AS episode_name,
           e.season, e.number,
           m.title AS movie_title,
           COALESCE(e.aired, m.release_date) AS aired,
           (SELECT source || ':' || path FROM images
            WHERE ((e.series_id IS NOT NULL AND series_id = e.series_id)
                OR (p.movie_id   IS NOT NULL AND movie_id  = p.movie_id))
              AND kind = 'poster' AND selected = 1 LIMIT 1) AS poster
    FROM pending p
    LEFT JOIN episodes e ON e.id = p.episode_id
    LEFT JOIN series   s ON s.id = e.series_id
    LEFT JOIN movies   m ON m.id = p.movie_id
    ORDER BY p.timestamp DESC, series_title, m.title
"#,
```

(The poster sub-select is awkward as one expression; an acceptable alternative
is two correlated sub-selects — one for the series poster, one for the movie
poster — wrapped in `COALESCE`. Pick whichever the implementor finds cleaner;
both produce the same result.)

**Methods — remove:** `set_series_next_episode`, `set_movie_pending`,
`pending_episodes`, `pending_movies`.

**Methods — add:**

```rust
// Manual pending (timestamp = now).
pub async fn add_pending_episode(&self, episode_id: EpisodeId, ts: Timestamp) -> Result<()>;
pub async fn add_pending_movie(&self, movie_id: MovieId, ts: Timestamp) -> Result<()>;
pub async fn remove_pending_episode(&self, episode_id: EpisodeId) -> Result<()>; // also called by MarkWatched
pub async fn remove_pending_movie(&self, movie_id: MovieId) -> Result<()>;

// Fill the pending slot for one series, but ONLY if it is currently empty.
// Used after a sync upserts a series' episodes, and after a MarkWatched
// deletes the previously-pending episode row.
// 1. has_pending_episode_for_series(series_id) — if a pending episode row
//    already exists for this series, return early and touch nothing
//    (manual pins and prior auto-picks are preserved by design).
// 2. otherwise SELECT next_pending_episode_for_series(series_id, today)
//    — the oldest aired (aired <= today), unwatched episode ordered by
//    season, number.
// 3. if a row is returned, upsert_pending_episode(aired.to_timestamp(), episode_id).
// Note: no bulk delete happens here anymore; this method never overwrites an
// existing pending entry, it only fills an empty slot.
pub async fn fill_pending_for_series(&self, series_id: SeriesId) -> Result<()>;

// Auto-discover movies past release that aren't pending yet (called after a
// movie sync, and once at startup — see section c option A).
// For each row from movies_needing_pending(today):
//   upsert_pending_movie(release_date.to_timestamp(), movie_id)
pub async fn discover_pending_movies(&self) -> Result<()>;

// Unified list (replaces pending_episodes + pending_movies).
pub async fn pending(&self) -> Result<Vec<api::Pending>>;
```

`pending()` runs `list_pending` and maps each `PendingRow` to `api::Pending`,
branching on `episode_id.is_some()` to build `PendingKind::Episode { series,
episode }` (label `S{season:02}E{number:02} – {name}`) vs
`PendingKind::Movie { movie }` (label = movie title). `series_title` is `Some`
for episodes, `None` for movies. `aired` carries the display date
(`COALESCE(e.aired, m.release_date)`).

Add the Date→Timestamp helper in `crates/api`:

```rust
impl Date {
    /// UTC midnight of this day as a Timestamp (for pending ordering).
    pub fn to_timestamp(self) -> Timestamp { /* CivilDate → midnight Zoned UTC → JiffTimestamp */ }
}
```

## c. PendingSystem — `crates/server/src/pending.rs`

All the business logic around maintaining the pending table lives in a single
new struct rather than being scattered across `ws.rs` and `sync.rs`:

```rust
pub(crate) struct PendingSystem {
    db: Database,
}

impl PendingSystem {
    pub fn new(db: Database) -> Self { Self { db } }

    /// Called after sync upserts a series' episodes.
    /// Fills the pending slot only if the series has no existing pending episode.
    pub async fn fill_for_series(&self, series_id: SeriesId) -> Result<()>;

    /// Called by the MarkWatched handler for episode watches.
    /// Removes the just-watched episode from pending, then fills the now-empty slot.
    pub async fn on_episode_watched(&self, series_id: SeriesId, episode_id: EpisodeId) -> Result<()>;

    /// Called after sync_movie and once at startup.
    /// Inserts pending rows for tracked movies whose release_date has passed and
    /// that are not yet watched or already pending.
    pub async fn discover_movies(&self) -> Result<()>;
}
```

`fill_for_series` and `on_episode_watched` both delegate to the DB methods
(`has_pending_episode_for_series`, `next_pending_episode_for_series`,
`upsert_pending_episode`, `remove_pending_episode`). `discover_movies` delegates
to `movies_needing_pending` + `upsert_pending_movie`.

`PendingSystem` is constructed in `main.rs` (it just wraps a `Database` clone)
and passed into `AppState` alongside `db`, `broadcast`, etc. — or passed directly
into `sync_series`/`sync_movie` as a parameter.

## d. Sync layer — `crates/server/src/sync.rs`

**Recommendation for "how do auto-discovered movies get into the table":
Option A — populate during sync (and once at startup).** Rationale:

- It mirrors how episodes are handled (recompute right after upserting the
  entity's data), so there is one consistent mental model: *the pending table is
  maintained as a side effect of sync.* The list query stays a dumb `SELECT …
  ORDER BY timestamp` with no write side effects — important because `ListPending`
  is read-only and called from multiple components; a lazy-populate-on-list
  (Option B) would make a read endpoint mutate the DB and would need the write
  lock on every dashboard load.
- Option C (keep a computed query for movies) is rejected because it reintroduces
  exactly the split-brain the refactor removes: some pending items in a table,
  others computed, two code paths, two sort mechanisms. The whole point is one
  table, one `timestamp` sort.
- The one gap with Option A: a movie whose `release_date` passes while nothing
  triggers a sync would not appear until the next sync. Close this with the
  **auto-sync loop already planned in §2 of plan.md** (which re-syncs everything
  on an interval) plus a one-shot `pending.discover_movies()` call at server
  startup in `main.rs`. That is sufficient for a single-user app; no separate
  scheduler needed.

Concrete changes:

- `sync_series` (the public entry, after both the tmdb/tvdb branch and before the
  final `PendingChanged` broadcast): call
  `pending.fill_for_series(series_id).await?;`. This only fills the pending
  slot when the series has **no** existing pending episode (it never deletes or
  overwrites an existing one — see section c). Both `sync_series_tmdb` and
  `sync_series_tvdb` already upsert all episodes before returning, so the fill
  sees fresh data and, for a brand-new series, selects the oldest unwatched aired
  episode.
- `sync_movie` (before the final `PendingChanged` broadcast): call
  `pending.discover_movies().await?;`. (Movies already watched or already
  pending are skipped by `movies_needing_pending`'s `NOT EXISTS` guards, so this
  is idempotent and cheap.)

**Manual episode pins are safe by design.** Because `fill_for_series`
only writes into an empty slot and never deletes or overwrites an existing
pending episode row, a manual pin set via `AddPending` (timestamp = `now()`)
survives every subsequent sync untouched. Auto-filled episodes use
`aired.to_timestamp()`; manual ones use `now()`. There is no recompute that can
clobber a deliberately-set entry.

## e. API types — `crates/api/src/lib.rs`

- `define_id!(PendingId);` (new).
- `Series`: remove `pending_episode_id` field. Update `series_from_row`.
- `Movie`: remove `pending` field. Update `movie_from_row`.
- Add `Date::to_timestamp` (see section b).
- `Pending` struct: unchanged shape works (`kind`, `aired`, `series_title`,
  `label`, `poster`). Optionally add `pub id: PendingId` and `pub timestamp:
  Timestamp` if the frontend wants to key/sort on them — but the dashboard
  currently keys on `kind`, so this is optional; add only if needed.
- **Requests:** replace the two single-purpose requests with a symmetric pair
  that writes the table directly:

```rust
pub struct AddPendingRequest    { pub kind: PendingKind }  // timestamp = now, server-side
pub struct RemovePendingRequest { pub kind: PendingKind }
```

  Remove `SetNextEpisodeRequest` and `SetMoviePendingRequest`. Reusing
  `PendingKind` (already `Episode { series, episode }` / `Movie { movie }`) means
  one request type serves both detail pages and matches the `watched` API's
  `WatchedKind` pattern. In the `api::define!` block, replace the `SetNextEpisode`
  and `SetMoviePending` endpoints with `AddPending` and `RemovePending` (both
  `Response = Empty`).

## f. Server WS handlers — `crates/server/src/ws.rs`

- Replace the `SetNextEpisode` handler with `AddPending`/`RemovePending`:

```rust
api::Request::AddPending => {
    let req = incoming.read::<api::AddPendingRequest>()?;
    let ts = api::Timestamp::now();                       // manual = now
    match req.kind {
        api::PendingKind::Episode { episode, .. } => db.add_pending_episode(episode, ts).await?,
        api::PendingKind::Movie   { movie }       => db.add_pending_movie(movie, ts).await?,
    }
    broadcast PendingChanged (channel = incoming.channel());
    outgoing.write(api::Empty);
}
api::Request::RemovePending => { /* symmetric, calls remove_pending_* */ }
```

- Delete the old `SetNextEpisode` and `SetMoviePending` handler arms. Note the
  old handlers also emitted `SeriesChanged`/`MovieChanged`; that was only needed
  to refresh the now-deleted `pending_episode_id`/`pending` fields, so it can be
  dropped — `PendingChanged` is the only event the dashboard needs.
- `MarkWatched` handler: after `db.mark_watched(...)`, when the request is for an
  **episode** (`WatchedKind::Episode { series, episode }`), advance the series'
  pending slot to the next episode:

```rust
api::WatchedKind::Episode { series, episode } => {
    pending.on_episode_watched(series, episode).await?;
}
```

  Do **not** do this for movie watches (`WatchedKind::Movie`). So `MarkWatched`
  for an episode becomes: mark watched -> `pending.on_episode_watched` (removes
  old pending row, fills next) -> broadcast `WatchedChanged` + `PendingChanged`.
- `RemoveWatched` handler: **unchanged.** It still just broadcasts
  `WatchedChanged` + `PendingChanged` and lets the frontend re-fetch. Removing a
  watch does not change which episode is "next" when a pending entry already
  exists, and if the series has no pending entry the next sync will fill it, so
  `RemoveWatched` does not call into `PendingSystem`.
- `ListPending` handler: replace the two-call
  `pending_episodes()` + `pending_movies()` with a single `db.pending().await?`.
- `ListWatchNext` handler: currently calls `pending_episodes()`. It wants only
  episode pending items. Either (a) call `db.pending().await?` and
  `retain(|p| matches!(p.kind, PendingKind::Episode { .. }))`, or (b) add a thin
  `db.pending_episodes_only()` variant of `list_pending` with
  `WHERE p.episode_id IS NOT NULL`. Prefer (a) — no extra statement.
- `RemoveSeries` / `RemoveMovie` handlers: no change needed —
  `ON DELETE CASCADE` removes pending rows automatically.

## g. Frontend

- **`series_detail.rs`:** `is_next` currently reads
  `series.pending_episode_id == Some(episode_id)`. That field is gone. The detail
  page does not load the pending table, so it has no cheap local source of truth
  for "is this episode pending." Simplest fix: drop the `is_next` highlight and
  the toggle's dependence on it — make the per-episode action an unconditional
  "Mark pending" that sends `AddPending { kind: Episode { series, episode } }`.
  The `Msg::SetNextEpisode(Option<EpisodeId>)` becomes
  `Msg::AddPending(EpisodeId)` (and optionally `Msg::RemovePending(EpisodeId)` if
  we keep an un-pin affordance). `SetNextEpisodeDone`'s local mutation of
  `series.pending_episode_id` (line ~511) is removed. If a visible pinned/unpinned
  toggle is still wanted, have the component fetch its own pending state via
  `ListPending` and filter for this series — but that is optional polish; the
  minimal change is the unconditional add.
- **`movie_detail.rs`:** `movie.pending` is gone. The "Mark pending / Clear
  pending" toggle (lines ~511-521) currently branches on `movie.pending`. Same
  treatment: without loading pending state the page can't know the current
  pending status cheaply. Minimal change: a single "Mark pending" button sending
  `AddPending { kind: Movie { movie } }` (and a "Remove pending" sending
  `RemovePending`). Replace `Msg::SetPending(bool)` /
  `SetMoviePendingRequest` usage (lines ~58, ~353, ~364) accordingly; drop the
  `movie.pending = pending` local mutation. Optionally fetch `ListPending` to
  drive a real toggle, as polish.
- **`dashboard.rs`, `watch_next.rs`, `queue.rs`:** consume `api::Pending` /
  `ListPendingResponse` unchanged (the `Pending` shape is preserved), so no
  changes beyond what §5 already covers. Sort order now comes purely from the
  server's `timestamp DESC`; these components already render in received order.

## h. Order of work

1. `crates/api`: `PendingId`, `Date::to_timestamp`, remove fields from
   `Series`/`Movie`, swap request types + endpoints.
2. Migration rewrite (`2026-06-05.sql`): drop columns, add `pending` table.
3. `crates/db`: row types, statements, methods; delete old pending code.
4. `crates/server/pending.rs`: new `PendingSystem` struct with `fill_for_series`,
   `on_episode_watched`, `discover_movies`; wire into `AppState` in `main.rs`.
5. `crates/server/sync.rs`: call `pending.fill_for_series` after series sync,
   `pending.discover_movies` after movie sync; call `pending.discover_movies` once
   at startup.
6. `crates/server/ws.rs`: `AddPending`/`RemovePending` handlers, `ListPending`
   single-call, `ListWatchNext` filter, `MarkWatched` calls `pending.on_episode_watched`.
7. Frontend `series_detail.rs` / `movie_detail.rs` button + message rewires.
8. `cargo check` all four crates; delete the dev DB so the rewritten migration
   applies cleanly (greenfield — no data to preserve).
