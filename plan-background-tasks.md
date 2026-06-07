# Plan: Smarter background sync + movie digital release tracking

This plan adds (1) a stale-sync background loop that only re-syncs series/movies
whose data is actually old, and (2) movie digital-release tracking so a movie
becomes "pending" when its **digital** release date passes — not only its
theatrical `release_date`.

It is self-contained: a developer who has not read the request can implement
from this document. Concrete struct shapes, SQL DDL, statement literals, and
method signatures are given. Design choices that need a decision are flagged
with a **Decision** note and a recommendation.

---

## Background facts about this codebase (verified against source)

These constrain the design; read before implementing.

- **`api::Date`** is stored in SQLite as an **INTEGER `YYYYMMDD`** (not TEXT).
  `FromColumn`/`BindValue` (in `crates/api/src/lib.rs` ~line 334) convert
  to/from `year*10000 + month*100 + day`. Two `Date`s compare correctly with
  integer `<`, `<=`. Over the wire (`Encode`/`Decode`) a `Date` is an ISO
  `"YYYY-MM-DD"` string, and `Date: FromStr` parses that string.
- **`api::Timestamp`** is stored as **INTEGER epoch milliseconds**
  (`as_millisecond()` / `from_millisecond`). It compares correctly with integer
  `<`. `Timestamp::now()` exists. There is **no** subtraction helper, but
  `Timestamp` wraps `jiff::Timestamp` (`.inner()` returns it), so a cutoff can
  be computed in Rust and bound as a normal value.
- **Migration policy** (greenfield, no back-compat): there is a single
  migration file `crates/db/migrations/2026-06-05.sql`. **Rewrite it in place**
  (add the new columns and table directly to the `CREATE TABLE` statements and
  add the new `movie_releases` table). Do **not** add a second migration file or
  stack `ALTER TABLE`s. Renaming the file's date is optional and cosmetic.
- **SQL rules**: never concatenate strings into a query; never build composite
  values inside SQL. Build composite values in Rust after fetching raw columns.
  Prefer simple, focused queries over big joins.
- **`statements!` macro** (`crates/db/src/lib.rs` ~line 178): every query is a
  named string literal inside `statements! { struct Inner { … } }` and is
  prepared once at startup. New SQL goes there. Each statement is used as
  `s.<name>.bind((…))?; … .next::<Row>()? / .step()?; s.<name>.reset()?;`.
- **`Date::today()`** and binding a `Date`/`Timestamp` directly into a statement
  both work (they implement `BindValue`).
- The `db` async methods follow a fixed shape: `lock_owned().await` then
  `spawn_blocking(move || { … })`.

---

## 1. Schema changes (`crates/db/migrations/2026-06-05.sql`)

### 1a. `last_synced_at` on `series` and `movies`

Add one nullable column (epoch-ms timestamp; `NULL` = never synced) to each
table's `CREATE TABLE`:

```sql
CREATE TABLE series (
    id             INTEGER PRIMARY KEY,
    title          TEXT NOT NULL,
    first_air      INTEGER,
    overview       TEXT NOT NULL DEFAULT '',
    tracked        INTEGER NOT NULL DEFAULT 1,
    sync_source    TEXT,
    last_synced_at INTEGER            -- epoch ms, NULL = never synced
);

CREATE TABLE movies (
    id             INTEGER PRIMARY KEY,
    title          TEXT NOT NULL,
    release_date   INTEGER,
    overview       TEXT NOT NULL DEFAULT '',
    tracked        INTEGER NOT NULL DEFAULT 1,
    sync_source    TEXT,
    last_synced_at INTEGER            -- epoch ms, NULL = never synced
);
```

Indexing `last_synced_at` is unnecessary — the tables are tiny (one row per
tracked title) and the stale query scans them fully.

### 1b. New `movie_releases` table

```sql
CREATE TABLE movie_releases (
    id           INTEGER PRIMARY KEY,
    movie_id     INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
    country      TEXT NOT NULL,        -- ISO 3166-1 alpha-2, e.g. "US"
    release_type INTEGER NOT NULL,     -- 1..=6 (see type table below)
    date         INTEGER NOT NULL,     -- YYYYMMDD, same encoding as other dates
    UNIQUE(movie_id, country, release_type)
);

CREATE INDEX idx_movie_releases_movie ON movie_releases (movie_id);
CREATE INDEX idx_movie_releases_digital
    ON movie_releases (date) WHERE release_type = 4;
```

**Decision — uniqueness key.** Use `UNIQUE(movie_id, country, release_type)`
(not `…, date`). Rationale: TMDB occasionally revises a release date for the same
country+type; we want the latest value to **replace** the old one via an upsert,
not accumulate duplicates. If `date` were part of the key, a revised date would
insert a second row and `discover` logic would see a stale earlier date. One row
per (movie, country, type) is the correct grain.

Release-type integers (TMDB): `1`=Premiere, `2`=Theatrical (limited),
`3`=Theatrical, `4`=Digital, `5`=Physical, `6`=TV. The partial index on
`release_type = 4` makes the "digital release passed" discovery query cheap.

---

## 2. `crates/api` changes

### 2a. `last_synced_at` on the wire structs

Add to `Series` (~line 762) and `Movie` (~line 837):

```rust
pub struct Series {
    // …existing fields…
    pub last_synced_at: Option<Timestamp>,
}

pub struct Movie {
    // …existing fields…
    pub last_synced_at: Option<Timestamp>,
}
```

`Timestamp` already implements `Encode`/`Decode`, so the derive handles the
wire format. Every place that constructs a `Series`/`Movie` literal (notably
`series_from_row`/`movie_from_row` in `db`) must add the field — the compiler
will point these out.

### 2b. `MovieRelease` and `ReleaseType`

**Decision — expose releases on the wire?** The first cut does **not** need a
full per-country release list on `Movie`; pending discovery is entirely
server-side. To keep the optional movie-detail UI (section 8) cheap *and* avoid
shipping raw integers to the frontend, add a small typed enum plus a compact
struct in `api`, but only populate it on the single-movie `GetMovie` path, not
on list endpoints.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[musli(mode = Binary)]               // match the derive style used elsewhere
pub enum ReleaseType {
    Premiere,            // 1
    TheatricalLimited,   // 2
    Theatrical,          // 3
    Digital,             // 4
    Physical,            // 5
    Tv,                  // 6
}

impl ReleaseType {
    pub fn from_tmdb(n: u8) -> Option<Self> {
        Some(match n {
            1 => Self::Premiere,
            2 => Self::TheatricalLimited,
            3 => Self::Theatrical,
            4 => Self::Digital,
            5 => Self::Physical,
            6 => Self::Tv,
            _ => return None,          // unknown types are dropped
        })
    }

    pub fn to_tmdb(self) -> u8 { /* inverse */ }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[musli(mode = Binary)]
pub struct MovieRelease {
    pub country: String,
    pub release_type: ReleaseType,
    pub date: Date,
}
```

(Check the exact derive attributes other `api` enums/structs use — e.g.
`SyncSource` at ~line 654 — and copy them verbatim so musli encoding matches.)

**Decision — put `releases` on `Movie`?** Recommended: add
`pub releases: Vec<MovieRelease>` to `Movie`, default empty. List endpoints
leave it empty (cheap); `movie_by_id` fills it. This mirrors how `remotes` and
`images` are empty on construction and filled by the detail query. If you prefer
to keep `Movie` unchanged, instead add a dedicated `GetMovieReleases` endpoint —
but adding to `Movie` is fewer moving parts and matches existing patterns, so
that is the recommendation. The unused-on-lists cost is one empty `Vec`.

---

## 3. `crates/db` changes

### 3a. Row types

Add a new row type and extend the two existing ones.

```rust
#[derive(Row)]
struct SeriesRow {
    id: SeriesId,
    title: String,
    first_air: Option<Date>,
    overview: String,
    tracked: bool,
    sync_source: Option<SyncSource>,
    last_synced_at: Option<Timestamp>,   // NEW
}

#[derive(Row)]
struct MovieRow {
    id: MovieId,
    title: String,
    release_date: Option<Date>,
    overview: String,
    watched: bool,
    watched_count: i64,
    tracked: bool,
    sync_source: Option<SyncSource>,
    last_synced_at: Option<Timestamp>,   // NEW
}

#[derive(Row)]
struct MovieReleaseRow {
    country: String,
    release_type: i64,                    // raw TMDB integer
    date: Date,
}
```

`Row` derives map columns positionally, so the **column order in every SELECT
must match the struct field order**. `last_synced_at` is added last on both
rows, so append it last in each SELECT (see 3c).

### 3b. `series_from_row` / `movie_from_row`

Add the new field:

```rust
fn series_from_row(r: SeriesRow) -> api::Series {
    api::Series { /* …existing… */ last_synced_at: r.last_synced_at }
}

fn movie_from_row(r: MovieRow) -> api::Movie {
    api::Movie { /* …existing… */ last_synced_at: r.last_synced_at, releases: Vec::new() }
}
```

`movie_by_id` then fills `releases` after building the movie (see 3e).

### 3c. Update every series/movie SELECT and INSERT…RETURNING

Append `last_synced_at` to the column list of each of these statements so the
positional `Row` mapping stays correct:

- `insert_series` — `RETURNING …, sync_source, last_synced_at` (a fresh row's
  value is `NULL`).
- `list_series`, `series_by_id`, `series_by_remote` — add `last_synced_at`
  (qualified `s.last_synced_at` where the query aliases `series s`).
- `insert_movie` — `RETURNING …, sync_source, last_synced_at`.
- `list_movies`, `movie_by_id`, `movie_by_remote` — add `m.last_synced_at`.

`update_series`/`update_movie` do **not** touch `last_synced_at` (sync timing is
set by a dedicated statement so a metadata edit doesn't reset the clock).

### 3d. New statements (add to `statements! { struct Inner { … } }`)

```rust
// last_synced_at writers
set_series_synced_at: r#"
    UPDATE series SET last_synced_at = ? WHERE id = ?
"#,
set_movie_synced_at: r#"
    UPDATE movies SET last_synced_at = ? WHERE id = ?
"#,

// stale-item selectors: never-synced OR synced before the cutoff
series_needing_sync: r#"
    SELECT id, title, first_air, overview, tracked, sync_source, last_synced_at
    FROM series
    WHERE tracked = 1
      AND (last_synced_at IS NULL OR last_synced_at < ?)
    ORDER BY last_synced_at IS NOT NULL, last_synced_at
"#,
movies_needing_sync: r#"
    SELECT m.id, m.title, m.release_date, m.overview,
           (SELECT COUNT(*) FROM watched w WHERE w.movie_id = m.id) > 0 AS watched,
           (SELECT COUNT(*) FROM watched w WHERE w.movie_id = m.id) AS watched_count,
           m.tracked, m.sync_source, m.last_synced_at
    FROM movies m
    WHERE m.tracked = 1
      AND (m.last_synced_at IS NULL OR m.last_synced_at < ?)
    ORDER BY m.last_synced_at IS NOT NULL, m.last_synced_at
"#,

// movie releases
upsert_movie_release: r#"
    INSERT INTO movie_releases (movie_id, country, release_type, date)
    VALUES (?, ?, ?, ?)
    ON CONFLICT(movie_id, country, release_type)
        DO UPDATE SET date = excluded.date
"#,
list_movie_releases: r#"
    SELECT country, release_type, date
    FROM movie_releases
    WHERE movie_id = ?
    ORDER BY date, country, release_type
"#,
digital_release_date_for_movie: r#"
    SELECT date FROM movie_releases
    WHERE movie_id = ? AND release_type = 4
    ORDER BY date
    LIMIT 1
"#,

// digital-release pending discovery (parallels movies_needing_pending)
movies_needing_pending_digital: r#"
    SELECT m.id, MIN(mr.date) AS release_date
    FROM movies m
    JOIN movie_releases mr ON mr.movie_id = m.id AND mr.release_type = 4
    WHERE m.tracked = 1
      AND mr.date <= ?
      AND NOT EXISTS (SELECT 1 FROM watched w WHERE w.movie_id = m.id)
      AND NOT EXISTS (SELECT 1 FROM pending p WHERE p.movie_id = m.id)
    GROUP BY m.id
"#,
```

Notes:
- The cutoff bound (`?`) for `series_needing_sync`/`movies_needing_sync` is a
  `Timestamp` computed in Rust (see 3f) — no arithmetic in SQL.
- `movies_needing_pending_digital` reuses the existing
  `PendingMovieCandidateRow { id, release_date: Option<Date> }` row type: it
  selects `id` and `MIN(date) AS release_date`. `MIN(date)` over a non-empty
  group is non-null; alias it `release_date` so the existing row type binds.
  Pick the **earliest** digital date so the pending timestamp sorts correctly.
- `ORDER BY last_synced_at IS NOT NULL, last_synced_at` puts never-synced items
  (NULL) first, then oldest-synced first — a sensible queue order.

### 3e. New async methods (on `Database`)

```rust
pub async fn set_series_synced_at(&self, id: SeriesId, at: Timestamp) -> Result<()>;
pub async fn set_movie_synced_at(&self, id: MovieId, at: Timestamp) -> Result<()>;

/// Replace/insert one release row.
pub async fn upsert_movie_release(
    &self, movie_id: MovieId, country: &str, release_type: u8, date: &Date,
) -> Result<()>;

/// All stored releases for a movie (used by movie_by_id and optional UI).
pub async fn movie_releases(&self, movie_id: MovieId) -> Result<Vec<api::MovieRelease>>;

/// Tracked series stale relative to `interval_hours`.
pub async fn series_needing_sync(&self, interval_hours: u32) -> Result<Vec<api::Series>>;

/// Tracked movies stale relative to `interval_hours`.
pub async fn movies_needing_sync(&self, interval_hours: u32) -> Result<Vec<api::Movie>>;
```

Implementation shape (each follows the existing `lock_owned + spawn_blocking`
pattern). For `movie_releases`, map each `MovieReleaseRow` with
`ReleaseType::from_tmdb(r.release_type as u8)` and **skip** rows whose type is
unknown (`None`). For `upsert_movie_release`, bind `release_type as i64`.

`series_needing_sync` / `movies_needing_sync` compute the cutoff in Rust before
the blocking closure:

```rust
let cutoff = cutoff_timestamp(interval_hours); // see 3f
let mut s = self.inner.clone().lock_owned().await;
spawn_blocking(move || {
    s.series_needing_sync.bind((cutoff,))?;
    let mut out = Vec::new();
    while let Some(r) = s.series_needing_sync.next::<SeriesRow>()? {
        out.push(series_from_row(r));
    }
    s.series_needing_sync.reset()?;
    Ok(out)
}).await?
```

These return full `api::Series`/`api::Movie`, but the background loop only needs
`id` + `title`; returning the full object keeps the method reusable and matches
the existing `series()`/`movies()` shape. They do **not** need to populate
`remotes`/`images`/`releases` (the loop ignores them); leave those empty as the
plain row mapping already does.

### 3f. Cutoff helper

Add a small free function in `crates/db/src/lib.rs`:

```rust
fn cutoff_timestamp(interval_hours: u32) -> api::Timestamp {
    let hours = interval_hours.max(1) as i64;
    // Timestamp wraps jiff::Timestamp; subtract the interval from now.
    let ts = api::Timestamp::now()
        .inner()
        .checked_sub(jiff::Span::new().hours(hours))
        .unwrap_or_else(|_| api::Timestamp::now().inner());
    // Wrap back into api::Timestamp.
    // If api::Timestamp has no public ctor from jiff::Timestamp, add one
    // (e.g. `Timestamp::from_jiff`) — it already has `.inner()` going the
    // other way, so add the symmetric constructor in `crates/api`.
    api::Timestamp::from_jiff(ts)
}
```

**Decision — add `Timestamp::from_jiff`.** `api::Timestamp` currently exposes
`now()` and `inner()` but no constructor from a `jiff::Timestamp`. Add
`pub fn from_jiff(ts: jiff::Timestamp) -> Self { Self(ts) }` in
`crates/api/src/lib.rs`. Alternative: compute the cutoff as raw epoch ms and
bind an `i64` — but binding a typed `Timestamp` keeps the statement signature
clean and is preferred. Either works because both encode to the same INTEGER.

### 3g. Update `discover_pending_movies` to include digital releases

**Decision — extend in place vs. add a sibling method.** Extend the existing
`discover_pending_movies` method to run **both** discovery queries in the same
blocking closure (theatrical via `movies_needing_pending`, digital via
`movies_needing_pending_digital`) and upsert pending rows for both candidate
sets. Rationale: callers (`sync_movie`, startup) already call
`discover_pending_movies` once; folding digital discovery in means no call-site
changes and one lock acquisition. A movie may legitimately match both queries —
`upsert_pending_movie` is idempotent (`ON CONFLICT(movie_id) DO UPDATE SET
timestamp = …`), so running both is safe; the digital pass, running second, sets
the timestamp to the digital `MIN(date).to_timestamp()`, which is the desired
sort key.

```rust
pub async fn discover_pending_movies(&self) -> Result<()> {
    let mut s = self.inner.clone().lock_owned().await;
    spawn_blocking(move || {
        let today = api::Date::today();

        // Pass 1: theatrical/general release_date (existing behaviour).
        s.movies_needing_pending.bind((today,))?;
        let mut candidates = Vec::new();
        while let Some(r) = s.movies_needing_pending.next::<PendingMovieCandidateRow>()? {
            candidates.push(r);
        }
        s.movies_needing_pending.reset()?;

        // Pass 2: digital release (release_type = 4) whose date has passed.
        s.movies_needing_pending_digital.bind((today,))?;
        while let Some(r) = s.movies_needing_pending_digital.next::<PendingMovieCandidateRow>()? {
            candidates.push(r);
        }
        s.movies_needing_pending_digital.reset()?;

        for r in candidates {
            let ts = r.release_date.map(|d| d.to_timestamp())
                .unwrap_or_else(api::Timestamp::now);
            s.upsert_pending_movie.bind((ts, r.id))?;
            ensure!(s.upsert_pending_movie.step()?.is_done(), "upsert_pending_movie");
            s.upsert_pending_movie.reset()?;
        }
        Ok(())
    }).await?
}
```

Note: a movie with both a passed theatrical and a passed digital date appears in
both passes; the second upsert wins and sets the digital-date timestamp. If a
movie has only a digital date in `movie_releases` but a NULL `movies.release_date`,
pass 2 still picks it up — this is exactly the new behaviour we want.

### 3h. `movie_by_id` fills `releases`

After building the movie (and its remotes/images), fetch releases and assign:

```rust
let releases = /* run list_movie_releases for this id, map rows */;
movie.releases = releases;
```

Keep `list_movies`/`movie_by_remote` releases empty (lists don't need them).

---

## 4. `crates/server/src/tmdb.rs`

### 4a. Fetch method

```rust
pub(crate) async fn fetch_movie_releases(&self, id: u32) -> Result<Vec<MovieReleaseInfo>> {
    #[derive(Deserialize)]
    struct Entry {
        #[serde(rename = "type")]
        type_: u8,
        #[serde(default)]
        release_date: Option<String>,
    }
    #[derive(Deserialize)]
    struct CountryBlock {
        iso_3166_1: String,
        #[serde(default)]
        release_dates: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Resp { #[serde(default)] results: Vec<CountryBlock> }

    let d: Resp = self.get_json(&format!("{BASE}/movie/{id}/release_dates")).await?;

    let mut out = Vec::new();
    for block in d.results {
        for e in block.release_dates {
            // type must be a known 1..=6; ignore otherwise
            if !(1..=6).contains(&e.type_) { continue; }
            // release_date is ISO 8601 with time; keep only the date part.
            if let Some(date) = parse_release_date(e.release_date.as_deref()) {
                out.push(MovieReleaseInfo {
                    country: block.iso_3166_1.clone(),
                    release_type: e.type_,
                    date,
                });
            }
        }
    }
    Ok(out)
}
```

Output type (in the "Output types" section of `tmdb.rs`):

```rust
pub(crate) struct MovieReleaseInfo {
    pub country: String,
    pub release_type: u8,   // 1..=6
    pub date: Date,
}
```

### 4b. Parsing the ISO-8601-with-time release date

The existing `opt_date` helper parses a bare `"YYYY-MM-DD"` via `Date: FromStr`,
which delegates to `CivilDate::from_str` and will **fail** on
`"2024-02-20T00:00:00.000Z"`. Add a helper that takes the first 10 chars:

```rust
fn parse_release_date(s: Option<&str>) -> Option<Date> {
    let s = s?.trim();
    if s.is_empty() { return None; }
    // TMDB returns e.g. "2024-02-20T00:00:00.000Z"; take the date prefix.
    let date_part = s.get(..10).unwrap_or(s);
    date_part.parse().ok()
}
```

(Do not reuse `opt_date` here — it would reject the time-bearing string.)

### 4c. `remote.rs` wrapper

Add a passthrough mirroring `fetch_tmdb_movie`:

```rust
pub(crate) async fn fetch_tmdb_movie_releases(&self, id: u32)
    -> Result<Vec<crate::tmdb::MovieReleaseInfo>>
{
    self.tmdb()
        .context("tmdb client not configured")?   // match existing error style
        .fetch_movie_releases(id)
        .await
}
```

(Use the same "no client configured" handling as `fetch_tmdb_movie` — copy that
method's exact shape.)

---

## 5. `crates/server/src/sync.rs`

### 5a. `sync_series` — stamp `last_synced_at`

Set the timestamp at the **end**, after TVMaze enrichment and after
`fill_for_series`, so the stamp reflects a fully completed sync. Add before the
final `PendingChanged` broadcast (order relative to that broadcast doesn't
matter):

```rust
pending.fill_for_series(series_id).await?;
db.set_series_synced_at(series_id, api::Timestamp::now()).await?;   // NEW
broadcast_event(broadcast, api::AppEventKind::PendingChanged);
```

**Decision — before or after TVMaze?** After. TVMaze enrichment is best-effort
and may be skipped, but the core TMDB/TVDB sync has already succeeded by the
time we reach this point; stamping at the very end means a stamped series is
fully up to date. (A TVMaze failure is logged and swallowed, so it does not
prevent stamping — acceptable.)

### 5b. `sync_movie` — fetch releases + stamp

Inside the `SyncSource::Tmdb` arm, after `update_movie` and image upserts and
before the `MovieChanged` broadcast, fetch and store releases. Then stamp at the
end. Releases fetch is best-effort (a missing endpoint shouldn't fail the whole
sync):

```rust
// after images, still inside the Tmdb arm:
match remote.fetch_tmdb_movie_releases(tmdb_id).await {
    Ok(releases) => {
        for r in releases {
            db.upsert_movie_release(movie_id, &r.country, r.release_type, &r.date).await?;
        }
    }
    Err(e) => warn!("movie release dates skipped for {movie_id}: {e:#}"),
}
```

At the end of `sync_movie`, after `discover_movies`:

```rust
pending.discover_movies().await?;
db.set_movie_synced_at(movie_id, api::Timestamp::now()).await?;    // NEW
broadcast_event(broadcast, api::AppEventKind::PendingChanged);
```

`discover_movies` now (per 3g) also checks digital releases, so a freshly
fetched digital date that is already in the past immediately produces a pending
entry on this same sync. Good.

---

## 6. Replace the background loop in `crates/server/src/main.rs`

Replace the existing loop (`main.rs` ~lines 103–140) with a **short-poll,
per-item-staleness** loop:

```rust
tokio::spawn({
    let db = db.clone();
    let queue = queue.clone();
    let broadcast = broadcast.clone();
    async move {
        // Poll cadence: how often we *check* for stale items. Independent of
        // the user-configured sync interval (which controls staleness).
        const POLL: std::time::Duration = std::time::Duration::from_secs(15 * 60);
        loop {
            tokio::time::sleep(POLL).await;

            let config = db.load_config().await.unwrap_or_default();
            if !config.auto_sync_enabled { continue; }
            let hours = config.auto_sync_interval_hours.max(1);

            for s in db.series_needing_sync(hours).await.unwrap_or_default() {
                queue.push(
                    api::TaskKind::SyncSeries { series_id: s.id, title: s.title },
                    false, &broadcast,
                ).await;
            }
            for m in db.movies_needing_sync(hours).await.unwrap_or_default() {
                queue.push(
                    api::TaskKind::SyncMovie { movie_id: m.id, title: m.title },
                    false, &broadcast,
                ).await;
            }
        }
    }
});
```

**Why a 15-minute poll instead of sleeping the full interval.** The old loop
slept `auto_sync_interval_hours` and then re-queued *everything*. Two problems it
fixes:
1. A series synced 1 hour ago was still re-queued on the next tick — wasteful
   API calls. Now `series_needing_sync(hours)` only returns items whose
   `last_synced_at` is older than the interval (or null).
2. A newly added series/movie had to wait up to a full interval (e.g. 24h)
   before the loop woke and considered it. With a 15-minute poll, a new item
   (which has `last_synced_at IS NULL`) is picked up within ~15 minutes.

The poll cadence (15 min) is a fixed constant, deliberately much shorter than
the minimum sensible sync interval (1h). It only does two cheap indexed table
scans per tick when enabled, and the `TaskQueue` dedup means an item already
queued/running won't be re-added. The 5-second inter-task delay in the queue
still paces the actual remote calls.

`std::time::Duration` is already referenced in `main.rs`; no new imports beyond
what is there. `api::TaskKind` is unchanged.

---

## 7. Pending discovery on digital release — summary

Covered by 3g + 5b. Behaviour after this change:

- A tracked, unwatched movie becomes pending when **either** its
  `movies.release_date` (theatrical/general) **or** its earliest
  `movie_releases` row with `release_type = 4` (digital) is `<= today`.
- The pending row's `timestamp` is the relevant date's `to_timestamp()`. When
  both apply, the digital pass runs second and sets the digital date (the more
  meaningful "available to watch" date) as the sort key.
- No change to series pending: `fill_pending_for_series` is already correct — it
  picks the oldest unwatched aired episode (`aired <= today`) when the series has
  no pending entry. **No update needed there.** (Verified in
  `crates/db/src/lib.rs` ~line 1329.)

---

## 8. Frontend (minimal, optional for a first cut)

All server-side; no new CSS (per CLAUDE.md). Both items below are optional and
can be deferred; do the data plumbing (sections 1–7) first.

### 8a. `last_synced_at` display (optional)

`api::Series`/`api::Movie` now carry `last_synced_at: Option<Timestamp>`. If
shown, render in the detail header sidebar of `series_detail.rs` /
`movie_detail.rs` as muted text, e.g. a `group text-muted` row reading
`"Last synced: {ts}"`. Use the existing `text-muted` class; no relative-time
formatting needed for a first cut (`Timestamp` Displays as RFC 3339). Skip if
not wanted — it has no functional effect.

### 8b. Movie release dates (optional)

`Movie::releases` is populated by `GetMovie`. On `movie_detail.rs`, optionally
add a `section` with `group text-muted` rows for the notable types (Theatrical,
Digital, Physical) for a chosen country (e.g. prefer `"US"`, else first
available), each row like `"Digital: 2024-02-20"`. Reuse the same classes as the
existing watch-history section. No new CSS.

---

## 9. Order of work

1. **Schema** — rewrite `crates/db/migrations/2026-06-05.sql`: add
   `last_synced_at` to `series` and `movies`; add `movie_releases` table +
   indexes (section 1). Delete the dev DB file so the migration re-runs.
2. **api** — add `last_synced_at` to `Series`/`Movie`; add `ReleaseType`,
   `MovieRelease`, and `Movie.releases`; add `Timestamp::from_jiff` (section 2,
   3f).
3. **db row types + from_row** — extend `SeriesRow`/`MovieRow`, add
   `MovieReleaseRow`; update `series_from_row`/`movie_from_row` (section 3a,3b).
4. **db SELECTs** — append `last_synced_at` to all series/movie SELECT and
   INSERT…RETURNING statements (section 3c). Compile; fix the positional row
   mismatches the compiler flags.
5. **db new statements + methods** — `set_*_synced_at`, `upsert_movie_release`,
   `movie_releases`, `series_needing_sync`, `movies_needing_sync`,
   `digital_release_date_for_movie`, `movies_needing_pending_digital`; the
   `cutoff_timestamp` helper (sections 3d–3f).
6. **db discover** — fold digital discovery into `discover_pending_movies`;
   fill `releases` in `movie_by_id` (sections 3g, 3h).
7. **tmdb/remote** — `fetch_movie_releases` + `MovieReleaseInfo` +
   `parse_release_date`; `remote.fetch_tmdb_movie_releases` (section 4).
8. **sync** — stamp `last_synced_at` in `sync_series`/`sync_movie`; fetch+upsert
   releases in `sync_movie` (section 5).
9. **main.rs** — replace the background loop with the 15-minute stale-poll loop
   (section 6).
10. **Build + smoke test**: `cargo build` all crates; run the server, add a
    movie with a known past digital release, trigger a sync, confirm a
    `movie_releases` row is stored and the movie appears in pending; confirm a
    just-synced item is not re-queued on the next poll while a never-synced one
    is.
11. **Frontend (optional)** — sections 8a/8b if desired.

---

## Open decisions recap (all have a recommended default)

- `movie_releases` uniqueness: **`(movie_id, country, release_type)`** ✓
- Expose releases on the wire: **yes, `Movie.releases`, filled only by
  `movie_by_id`** ✓
- `ReleaseType` enum vs raw int on the wire: **enum** ✓
- Extend `discover_pending_movies` vs new method: **extend in place** ✓
- Stamp `last_synced_at` after TVMaze: **yes, at the very end** ✓
- Poll cadence: **fixed 15 min, staleness from config interval** ✓
- Add `Timestamp::from_jiff`: **yes** (or bind raw i64 cutoff) ✓
