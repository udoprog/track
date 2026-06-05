# ontv-musli-web — Implementation Plan

## 1. Objective

Convert the existing **ontv** desktop application (an [Iced](https://iced.rs)
GUI TV-show / movie tracker that keeps data in in-memory `HashMap`s serialized
to YAML) into a **web application** structured *exactly* like the **territory**
template.

The result is a single-user, local web app that:

- Stores all data in **SQLite** via [`sqll`](https://crates.io/crates/sqll).
- Communicates between browser and server over a **`musli-web` WebSocket RPC**
  channel. All API request/response types are defined once in a shared `api`
  crate using `musli-core` `Encode`/`Decode`.
- Drives reactive UI updates through a **global broadcast event stream**
  (`AppEvent` / `AppEventKind`). Every mutation on the server broadcasts an
  event; every component subscribes and patches its own local state.
- Uses **data-owning components**: each Yew component loads the data it
  displays by issuing its own WS request when its channel opens, and keeps that
  data up to date by listening to broadcasts. **We never prop-drill entity data
  via `fn changed` / `Properties`.** Props are limited to identifiers,
  routing, and callbacks.
- Proxies poster / banner / fanart images from TMDB and TVDB through the server.
- Runs TMDB/TVDB metadata sync as **tokio background tasks** triggered by a WS
  request.
- Has **no authentication** — it is a single-user local app. All of territory's
  auth/session/login/google-oauth/registration machinery is dropped.

The visual layer reuses territory's CSS class vocabulary verbatim (`app`,
`app-body`, `toolbar`, `row`, `row-fill`, `section`, `fill`, `btn`, `btn-icon`,
`btn-icon-success`, `btn-icon-danger`, `input-group`, `input-text`, `title`,
`empty`, `section-header`, `icon <name>`, `icon-inline`). **Do not invent new
CSS classes** unless a layout genuinely cannot be expressed with the existing
set.

### Source → target mapping at a glance

| ontv (Iced)                              | ontv-musli-web (territory stack)                       |
| ---------------------------------------- | ------------------------------------------------------ |
| `HashMap` DBs + YAML files               | SQLite via `sqll`, `statements!` macro                 |
| UUID newtypes (`SeriesId(Uuid)` …)       | `u64` newtypes via `define_id!` (SQLite `INTEGER` PK)  |
| `chrono::NaiveDate` / `DateTime<Utc>`    | `NaiveDate` as `TEXT`, times as `jiff::Timestamp` TEXT |
| Iced `Message` / `update` / `view`       | Yew `Component` (`create`/`update`/`view`)             |
| `comps::*` reusable widgets w/ messages  | Yew child components, data-owning where they show data |
| `page::*` (dashboard, queue, …)          | Yew page components behind a history router            |
| `service`/`database` sync to TMDB/TVDB   | tokio background tasks behind a WS sync endpoint        |
| In-app image cache                       | server-side image proxy `/api/image/{source}/{path}`   |

---

## 2. Workspace structure

Mirror territory exactly, minus the map-specific (`lantmateriet`) crate and the
auth surface.

```
ontv-musli-web/
├── Cargo.toml                      # [workspace] members = ["crates/*"], resolver = "2"
├── Trunk.toml                      # frontend build config (copy from territory)
├── plan.md                         # this file
├── .gitignore                      # target/, dist/, *.db
├── crates/
│   ├── api/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs              # all shared types + api::define! block
│   ├── db/
│   │   ├── Cargo.toml
│   │   ├── migrations/
│   │   │   └── 2026-06-05.sql      # full schema (single migration to start)
│   │   └── src/lib.rs              # Database struct + statements! + async methods
│   ├── server/                     # was `territory`
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs             # Args (clap), AppState, Router, startup
│   │       ├── ws.rs               # WsHandler: ws::Handler impl, ws_handler upgrade fn
│   │       ├── proxy.rs            # /api/image/{source}/{path} image proxy
│   │       ├── cache.rs            # on-disk image cache (port territory cache.rs)
│   │       ├── sync.rs             # TMDB/TVDB sync background tasks
│   │       ├── tmdb.rs             # TMDB reqwest client (port ontv api/themoviedb.rs)
│   │       ├── tvdb.rs             # TVDB reqwest client (port ontv api/thetvdb.rs)
│   │       ├── error.rs            # axum error helpers (port territory error.rs)
│   │       └── static_assets.rs    # embed ../../dist behind `bundle` feature
│   └── frontend/                   # was `frontend`
│       ├── Cargo.toml
│       ├── index.html              # copy from territory (drop map-only bits)
│       ├── favicon.png
│       ├── style/
│       │   └── main.scss           # reuse territory's scss (+ _icons.scss)
│       └── src/
│           ├── lib.rs              # module decls + wasm-bindgen start
│           ├── root.rs             # Root component (no auth: always App)
│           ├── app.rs              # toolbar/layout shell, hosts the router pages
│           ├── router.rs           # Route enum + RouterState (history API)
│           ├── setup_channel.rs    # copy verbatim from territory
│           ├── error.rs            # copy from territory
│           ├── ui.rs               # ConfirmDanger, ErrorBanner, small shared UI
│           ├── http.rs             # fetch /api/config (no auth/me)
│           ├── image.rs            # <Poster>/<Image> helper -> proxy url
│           ├── dashboard.rs        # page: pending + schedule
│           ├── queue.rs            # page: ordered pending list
│           ├── search.rs           # page: TMDB/TVDB search
│           ├── series_list.rs      # page: tracked series
│           ├── series.rs           # page: series detail (seasons)
│           ├── season.rs           # page: season detail (episodes)
│           ├── movies_list.rs      # page: tracked movies
│           ├── movie.rs            # page: movie detail
│           ├── settings.rs         # page: config editor
│           ├── watch_next.rs       # page: what to watch next
│           ├── series_banner.rs    # data-owning component (loads its series)
│           ├── movie_banner.rs     # data-owning component (loads its movie)
│           ├── episode_row.rs      # episode + watch controls
│           ├── watch_button.rs     # mark watched / remove watch controls
│           └── calendar.rs         # schedule calendar widget (presentational)
```

> Naming note: territory's server binary crate is named `territory`; here it is
> named `server` (binary `ontv`). The frontend `RustEmbed` folder in
> `static_assets.rs` stays `../../dist`. Keep the workspace `Cargo.toml`
> wildcard members so new crates are picked up automatically.

---

## 3. Crate: `api`

`crates/api/src/lib.rs` — shared types and the `api::define!` endpoint block.
Modeled on territory's `api` crate. `Cargo.toml` features: `sqll = ["dep:sqll"]`.

### 3.1 ID newtypes

Copy territory's `define_id!` macro verbatim (`u64` wrapper, `Encode`/`Decode`,
`serde` transparent, `Display` as hex, and — behind `feature = "sqll"` —
`FromColumn<Type = Integer>` + `BindValue`). Then:

```rust
define_id!(SeriesId);
define_id!(EpisodeId);
define_id!(SeasonId);   // NEW surrogate id (ontv seasons had no UUID)
define_id!(MovieId);
define_id!(WatchedId);
```

`TaskId` from ontv is dropped — background sync tasks are tracked server-side by
`tokio::task` handles, not exposed to the API.

### 3.2 Timestamp

Copy territory's `Timestamp(jiff::Timestamp)` newtype verbatim (TEXT-backed,
RFC-3339). Used for `Watched.timestamp` and any sync bookkeeping.

### 3.3 Date

ontv uses `chrono::NaiveDate` for air/release dates. In `api`, model these as a
small `Date` newtype stored as `TEXT` (`YYYY-MM-DD`). Implement `FromStr` /
`Display` (ISO format), `Encode`/`Decode` (transparent over `String` or `(i32
year, u8 month, u8 day)` — prefer a `String` field for simplicity), and behind
`feature = "sqll"` `FromColumn<Type = Text>` + `BindValue`. All "date" fields
below are `Option<Date>`.

> Keep it dumb: `Date` wraps a validated `String` parsed lazily by the frontend
> with `jiff::civil::Date::strptime`/`from_str`. Do not pull `chrono` into the
> web stack.

### 3.4 String-typed remote ids and images (stored as TEXT)

Per the brief, **do not** model `RemoteId` / `RemoteEpisodeId` / `ImageV2` as
musli enums with payloads. Model them as **string newtypes** that round-trip
the ontv `Display`/`FromStr` format and store as `TEXT`:

```rust
// "tvdb:123", "tmdb:456", "imdb:tt0000001"
pub struct RemoteId(String);
// "tvdb:/posters/abc.jpg", "tmdb:/xY.jpg"
pub struct Image(String);
```

Each gets:
- `Encode`/`Decode` transparent over `String`.
- `serde` transparent (used by the reqwest sync clients).
- Helper accessors: `RemoteId::source() -> &str` (`"tvdb"|"tmdb"|"imdb"`),
  `RemoteId::value() -> &str`; `RemoteId::tvdb(u32)`, `tmdb(u32)`,
  `imdb(&str)` constructors; `Image::source()` / `Image::path()` and
  `Image::tvdb(path)` / `Image::tmdb(path)` constructors.
- `Image::proxy_url(&self) -> String` returning
  `/api/image/{source}/{path}` for the frontend.
- Behind `feature = "sqll"`: `FromColumn<Type = Text>` + `BindValue`.

This keeps the SQLite column a plain `TEXT` and avoids encoding enum variants.

### 3.5 SeasonNumber

```rust
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, ...)]
pub enum SeasonNumber { Specials, Number(u32) }
```

- `Encode`/`Decode` derive (musli handles the enum natively over the wire).
- For SQLite we **do not** store the enum directly. The `seasons.number` and
  `episodes.season` columns are `INTEGER`, where **`Specials` → `0`** and
  **`Number(n)` → `n`** (regular seasons start at 1 in TVDB/TMDB, so 0 is free
  for specials). Provide:
  - `SeasonNumber::to_i64(self) -> i64` (`Specials => 0`, `Number(n) => n`).
  - `SeasonNumber::from_i64(i64) -> SeasonNumber` (`0 => Specials`, `n =>
    Number(n)`).
  These are used by the `db` crate's row mapping; `SeasonNumber` itself does not
  implement `FromColumn`/`BindValue` (the db maps via the `i64` helpers).
- `Display`: `Specials => "Specials"`, `Number(n) => "Season {n}"`; plus a
  `short()` helper (`"S"` / `"{n}"`) like ontv.

### 3.6 ThemeType

```rust
pub enum ThemeType { Light, Dark }   // Encode/Decode, default Dark
```

### 3.7 Core data types (`Encode`/`Decode`, `Clone`, `Debug`)

These are the wire/UI shapes. They are *flattened* relative to ontv (no nested
`graphics` sub-structs holding alternative-image sets; we keep only the active
poster/banner/fanart that the UI needs).

```rust
pub struct Series {
    pub id: SeriesId,
    pub title: String,
    pub first_air_date: Option<Date>,
    pub overview: String,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub fanart: Option<Image>,
    pub tracked: bool,
    pub remote_id: Option<RemoteId>,
}

pub struct Season {
    pub id: SeasonId,
    pub series_id: SeriesId,
    pub number: SeasonNumber,
    pub air_date: Option<Date>,
    pub name: Option<String>,
    pub overview: String,
    pub poster: Option<Image>,
}

pub struct Episode {
    pub id: EpisodeId,
    pub series_id: SeriesId,
    pub season: SeasonNumber,
    pub number: u32,
    pub absolute_number: Option<u32>,
    pub name: Option<String>,
    pub overview: String,
    pub aired: Option<Date>,
    pub filename: Option<Image>,   // episode still
    pub remote_id: Option<RemoteId>,
    pub watched: bool,             // derived (any Watched row for this episode)
    pub watched_count: u32,        // derived count, for "watched N times"
}

pub struct Movie {
    pub id: MovieId,
    pub title: String,
    pub release_date: Option<Date>,
    pub overview: String,
    pub poster: Option<Image>,
    pub banner: Option<Image>,
    pub fanart: Option<Image>,
    pub remote_id: Option<RemoteId>,
    pub watched: bool,
    pub watched_count: u32,
}

pub enum WatchedKind {
    Series { series: SeriesId, episode: EpisodeId },
    Movie  { movie: MovieId },
}

pub struct Watched {
    pub id: WatchedId,
    pub timestamp: Timestamp,
    pub kind: WatchedKind,
}

// Pending = computed (no table). What is left to watch next.
pub enum PendingKind {
    Episode { series: SeriesId, episode: EpisodeId },
    Movie   { movie: MovieId },
}

pub struct Pending {
    pub timestamp: Timestamp,   // air/release date as ts, used for ordering
    pub kind: PendingKind,
    // Denormalized fields so the dashboard/queue render without N extra queries:
    pub series_title: Option<String>,
    pub label: String,          // e.g. "S01E03 — Episode name" or movie title
    pub poster: Option<Image>,
}

pub struct Config {
    pub theme: ThemeType,
    pub tvdb_legacy_apikey: String,
    pub tmdb_api_key: String,
    pub schedule_duration_days: u32,
    pub dashboard_limit: u32,
    pub dashboard_page: u32,
    pub schedule_limit: u32,
    pub schedule_page: u32,
}
```

Search results (returned from the live TMDB/TVDB search endpoint, not stored):

```rust
pub struct SearchSeries {
    pub remote_id: RemoteId,
    pub title: String,
    pub poster: Option<Image>,
    pub overview: String,
    pub first_air_date: Option<Date>,
    pub already_tracked: Option<SeriesId>, // Some if we already have this remote
}

pub struct SearchMovie {
    pub remote_id: RemoteId,
    pub title: String,
    pub poster: Option<Image>,
    pub overview: String,
    pub release_date: Option<Date>,
    pub already_tracked: Option<MovieId>,
}

pub enum SearchKind { Series, Movies }
```

Schedule (for the dashboard calendar):

```rust
pub struct ScheduledDay {
    pub date: Date,
    pub entries: Vec<ScheduledEntry>,
}
pub struct ScheduledEntry {
    pub series_id: SeriesId,
    pub series_title: String,
    pub episodes: Vec<Episode>,
}
```

`Empty` marker struct (copy territory's).

### 3.8 Request / Response structs

Define one `*Request` (and `*Response` where the reply is non-trivial) struct
per endpoint. List below; each derives `Encode, Decode, Debug`.

**Series**
- `ListSeriesRequest` → `ListSeriesResponse { series: Vec<Series> }`
  (all tracked series; for `series_list` page).
- `GetSeriesRequest { id: SeriesId }` → `GetSeriesResponse { series: Series }`.
- `ListSeasonsRequest { series_id: SeriesId }` →
  `ListSeasonsResponse { seasons: Vec<Season> }`.
- `TrackSeriesRequest { remote_id: RemoteId }` → `Series` (add from search /
  enable tracking; triggers an initial sync, see §9).
- `UntrackSeriesRequest { id: SeriesId }` → `Empty`.
- `RemoveSeriesRequest { id: SeriesId }` → `Empty` (delete entirely).

**Seasons / Episodes**
- `ListEpisodesRequest { series_id: SeriesId, season: SeasonNumber }` →
  `ListEpisodesResponse { episodes: Vec<Episode> }`.
- `GetEpisodeRequest { id: EpisodeId }` → `GetEpisodeResponse { episode: Episode }`.

**Movies**
- `ListMoviesRequest` → `ListMoviesResponse { movies: Vec<Movie> }`.
- `GetMovieRequest { id: MovieId }` → `GetMovieResponse { movie: Movie }`.
- `TrackMovieRequest { remote_id: RemoteId }` → `Movie`.
- `RemoveMovieRequest { id: MovieId }` → `Empty`.

**Watched**
- `MarkWatchedRequest { kind: WatchedKind, timestamp: Option<Timestamp> }`
  → `MarkWatchedResponse { watched: Watched }`.
- `RemoveWatchedRequest { id: WatchedId }` → `Empty`.
- `ListWatchedRequest { kind: WatchedKind }`  (history for one entity)
  → `ListWatchedResponse { watched: Vec<Watched> }`.

**Pending / Queue / Schedule / WatchNext**
- `ListPendingRequest` → `ListPendingResponse { pending: Vec<Pending> }`.
- `ListScheduleRequest { days: u32 }` →
  `ListScheduleResponse { days: Vec<ScheduledDay> }`.
- `ListWatchNextRequest` → `ListWatchNextResponse { pending: Vec<Pending> }`.

**Search / Sync**
- `SearchRequest { kind: SearchKind, query: String }` →
  `SearchResponse { series: Vec<SearchSeries>, movies: Vec<SearchMovie> }`.
- `SyncSeriesRequest { id: SeriesId }` → `Empty` (kick off background refresh).
- `SyncAllRequest` → `Empty` (refresh everything tracked).

**Config**
- `GetConfigRequest` → `GetConfigResponse { config: Config }`.
- `SetConfigRequest { config: Config }` → `Empty`.

### 3.9 Broadcast events

```rust
pub struct AppEvent { pub channel: ChannelId, pub kind: AppEventKind }

pub enum AppEventKind {
    SeriesCreated   { series: Series },
    SeriesChanged   { series: Series },
    SeriesDeleted   { series_id: SeriesId },

    SeasonsChanged  { series_id: SeriesId, seasons: Vec<Season> },

    EpisodeChanged  { episode: Episode },              // watched flag flips, metadata
    EpisodesChanged { series_id: SeriesId, season: SeasonNumber }, // bulk; listeners reload

    MovieCreated    { movie: Movie },
    MovieChanged    { movie: Movie },
    MovieDeleted    { movie_id: MovieId },

    WatchedChanged  { kind: WatchedKind },             // a watch was added/removed
    PendingChanged,                                    // recompute pending/queue/schedule

    ConfigChanged   { config: Config },
    SyncStarted     { series_id: Option<SeriesId> },
    SyncFinished    { series_id: Option<SeriesId> },
}
```

Granularity rule of thumb: emit a *specific* event (with the new entity) when a
single row changes so listeners can patch in place; emit a *coarse* event
(`EpisodesChanged`, `PendingChanged`) when many rows change so listeners simply
re-issue their list request. Pending/queue/schedule/watch-next pages all listen
for `WatchedChanged` and `PendingChanged` and reload.

### 3.10 `api::define!` block

One entry per endpoint plus the broadcast, exactly in territory's style:

```rust
api::define! {
    pub type ListSeries;
    impl Endpoint for ListSeries {
        impl Request for ListSeriesRequest;
        type Response<'de> = ListSeriesResponse;
    }
    // ... GetSeries, ListSeasons, TrackSeries, UntrackSeries, RemoveSeries,
    //     ListEpisodes, GetEpisode,
    //     ListMovies, GetMovie, TrackMovie, RemoveMovie,
    //     MarkWatched, RemoveWatched, ListWatched,
    //     ListPending, ListSchedule, ListWatchNext,
    //     Search, SyncSeries, SyncAll,
    //     GetConfig, SetConfig ...

    pub type AppBroadcast;
    impl Broadcast for AppBroadcast {
        impl Event for AppEvent;
    }
}
```

The macro generates `api::Request` (the `WsHandler::Id` enum with one variant
per endpoint, plus `Unknown`) and the typed `Endpoint`/`Broadcast` machinery.

---

## 4. Crate: `db` — SQLite schema (single migration)

`crates/db/migrations/2026-06-05.sql`. SQLite, `INTEGER PRIMARY KEY` rowids map
to `u64` ids via territory's `define_id!` (`i64.cast_unsigned()`). All dates and
remote ids and images are `TEXT`. `season`/`number` columns are `INTEGER`.

```sql
CREATE TABLE series (
    id            INTEGER PRIMARY KEY,
    title         TEXT NOT NULL,
    first_air_date TEXT,                      -- 'YYYY-MM-DD'
    overview      TEXT NOT NULL DEFAULT '',
    poster        TEXT,                        -- 'tvdb:/...'/'tmdb:/...'
    banner        TEXT,
    fanart        TEXT,
    tracked       INTEGER NOT NULL DEFAULT 1,  -- bool
    remote_id     TEXT                         -- 'tvdb:123' etc, UNIQUE-ish
);
CREATE UNIQUE INDEX series_remote_id ON series (remote_id) WHERE remote_id IS NOT NULL;

CREATE TABLE seasons (
    id         INTEGER PRIMARY KEY,            -- surrogate (ontv had none)
    series_id  INTEGER NOT NULL REFERENCES series (id) ON DELETE CASCADE,
    number     INTEGER NOT NULL,               -- 0 = Specials, n = Number(n)
    air_date   TEXT,
    name       TEXT,
    overview   TEXT NOT NULL DEFAULT '',
    poster     TEXT,
    UNIQUE (series_id, number)
);

CREATE TABLE episodes (
    id              INTEGER PRIMARY KEY,
    series_id       INTEGER NOT NULL REFERENCES series (id) ON DELETE CASCADE,
    season          INTEGER NOT NULL,          -- season number, 0 = Specials
    number          INTEGER NOT NULL,          -- episode number within season
    absolute_number INTEGER,
    name            TEXT,
    overview        TEXT NOT NULL DEFAULT '',
    aired           TEXT,                       -- 'YYYY-MM-DD'
    filename        TEXT,                       -- episode still image
    remote_id       TEXT,
    UNIQUE (series_id, season, number)
);
CREATE INDEX episodes_series ON episodes (series_id);
CREATE INDEX episodes_aired  ON episodes (aired);

CREATE TABLE movies (
    id           INTEGER PRIMARY KEY,
    title        TEXT NOT NULL,
    release_date TEXT,
    overview     TEXT NOT NULL DEFAULT '',
    poster       TEXT,
    banner       TEXT,
    fanart       TEXT,
    remote_id    TEXT
);
CREATE UNIQUE INDEX movies_remote_id ON movies (remote_id) WHERE remote_id IS NOT NULL;

-- One row per watch event (allows "watched N times" + history)
CREATE TABLE watched (
    id         INTEGER PRIMARY KEY,
    timestamp  TEXT NOT NULL,                  -- jiff RFC-3339
    series_id  INTEGER REFERENCES series (id)  ON DELETE CASCADE,
    episode_id INTEGER REFERENCES episodes (id) ON DELETE CASCADE,
    movie_id   INTEGER REFERENCES movies (id)  ON DELETE CASCADE,
    CHECK (
        (episode_id IS NOT NULL AND series_id IS NOT NULL AND movie_id IS NULL) OR
        (movie_id   IS NOT NULL AND episode_id IS NULL    AND series_id IS NULL)
    )
);
CREATE INDEX watched_episode ON watched (episode_id);
CREATE INDEX watched_movie   ON watched (movie_id);

CREATE TABLE config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Notes:
- **No `pending` table.** "Pending" = aired-but-unwatched episodes of tracked
  series + released-but-unwatched tracked movies. It is a computed query (§5).
- `tracked` defaults to 1; untracking sets it 0 but keeps rows (so watch history
  survives). `RemoveSeries`/`RemoveMovie` delete rows (cascades clean up).
- Migration runner / `migrations` table / `do_migrations` / `ensure_mode` are
  copied verbatim from territory's `db/src/lib.rs`.

---

## 5. Crate: `db` — `Database` struct and methods

Copy territory's scaffolding verbatim: `Migrations` `RustEmbed`, the
`statements!` macro, `Database { inner: Arc<Mutex<Inner>> }`, `open()`,
`do_migrations`, `ensure_mode`. Each public method follows the territory shape:
`lock_owned().await` → `spawn_blocking(move || { bind; step/next; map rows })`
→ `result.await?`.

Define `#[derive(Row)]` structs for each select shape (e.g. `SeriesRow`,
`SeasonRow`, `EpisodeRow`, `MovieRow`, `WatchedRow`, plus aggregate rows that
include a `watched_count`). Map them to `api::*` types with small `*_from_row`
helpers, using `SeasonNumber::from_i64` for the `season`/`number` columns.

### Statements (in the `statements! { struct Inner { … } }` block)

**Series**
- `list_series`: `SELECT … FROM series WHERE tracked = 1 ORDER BY title`.
- `series_by_id`: `SELECT … FROM series WHERE id = ?`.
- `series_by_remote`: `SELECT … FROM series WHERE remote_id = ?`.
- `insert_series`: `INSERT INTO series (...) VALUES (...) RETURNING ...`.
- `upsert_series`: `INSERT INTO series (remote_id, ...) VALUES (...) ON CONFLICT(remote_id) DO UPDATE SET title=…, overview=…, poster=…, … RETURNING ...` (used by sync).
- `set_series_tracked`: `UPDATE series SET tracked = ? WHERE id = ?`.
- `delete_series`: `DELETE FROM series WHERE id = ?`.

**Seasons**
- `list_seasons`: `SELECT … FROM seasons WHERE series_id = ? ORDER BY number`.
- `upsert_season`: `INSERT … ON CONFLICT(series_id, number) DO UPDATE … RETURNING …`.
- `delete_seasons_for_series`: `DELETE FROM seasons WHERE series_id = ?` (sync replace).

**Episodes** (note: `watched` / `watched_count` derived via correlated subquery
or `LEFT JOIN watched`)
- `list_episodes`:
  ```sql
  SELECT e.*, COUNT(w.id) AS watched_count
  FROM episodes e
  LEFT JOIN watched w ON w.episode_id = e.id
  WHERE e.series_id = ? AND e.season = ?
  GROUP BY e.id
  ORDER BY e.number
  ```
- `episode_by_id`: same shape, `WHERE e.id = ?`.
- `upsert_episode`: `INSERT … ON CONFLICT(series_id, season, number) DO UPDATE … RETURNING …`.
- `delete_episodes_for_series`: `DELETE FROM episodes WHERE series_id = ?`.

**Movies**
- `list_movies`: `SELECT m.*, COUNT(w.id) … LEFT JOIN watched … GROUP BY m.id ORDER BY title`.
- `movie_by_id`, `movie_by_remote`, `insert_movie`, `upsert_movie`,
  `delete_movie` — analogous to series.

**Watched**
- `insert_watched_episode`: `INSERT INTO watched (timestamp, series_id, episode_id) VALUES (?, ?, ?) RETURNING id, timestamp`.
- `insert_watched_movie`: `INSERT INTO watched (timestamp, movie_id) VALUES (?, ?) RETURNING id, timestamp`.
- `delete_watched`: `DELETE FROM watched WHERE id = ?`.
- `watched_for_episode`: `SELECT id, timestamp FROM watched WHERE episode_id = ? ORDER BY timestamp DESC`.
- `watched_for_movie`: `SELECT id, timestamp FROM watched WHERE movie_id = ? ORDER BY timestamp DESC`.

**Pending / Schedule / Watch-next** (computed)
- `list_pending` — first unwatched aired episode per tracked series, plus
  released unwatched movies, ordered by air/release date. Sketch:
  ```sql
  SELECT s.id AS series_id, e.id AS episode_id, e.aired, s.title,
         e.season, e.number, e.name, s.poster
  FROM series s
  JOIN episodes e ON e.series_id = s.id
  WHERE s.tracked = 1
    AND e.aired IS NOT NULL AND e.aired <= ?           -- today
    AND NOT EXISTS (SELECT 1 FROM watched w WHERE w.episode_id = e.id)
    AND e.id = (                                        -- earliest unwatched per series
        SELECT e2.id FROM episodes e2
        WHERE e2.series_id = s.id
          AND e2.aired IS NOT NULL AND e2.aired <= ?
          AND NOT EXISTS (SELECT 1 FROM watched w2 WHERE w2.episode_id = e2.id)
        ORDER BY e2.season, e2.number LIMIT 1)
  ORDER BY e.aired
  ```
  Movies: `SELECT … FROM movies m WHERE release_date <= ? AND NOT EXISTS(watched)`.
  Combine both result sets in Rust into `Vec<Pending>` (build `label`,
  `series_title`, `poster`).
- `list_schedule` — upcoming (not-yet-aired) episodes within `days`:
  `WHERE e.aired > ? AND e.aired <= ?` grouped by date in Rust into
  `Vec<ScheduledDay>`.
- `list_watch_next` — like `list_pending` but may include the *next* unwatched
  episode even if it hasn't aired yet, per ontv's WatchNext semantics; reuse the
  pending query relaxed on the `aired <= today` constraint.

**Config**
- `get_config` / `set_config` — key/value, identical to territory.

### `Database` public async methods (signatures)

```rust
// series
async fn list_series(&self) -> Result<Vec<api::Series>>;
async fn series(&self, id: SeriesId) -> Result<Option<api::Series>>;
async fn upsert_series(&self, ...) -> Result<api::Series>;       // sync
async fn set_series_tracked(&self, id: SeriesId, tracked: bool) -> Result<()>;
async fn delete_series(&self, id: SeriesId) -> Result<()>;
// seasons
async fn seasons(&self, series_id: SeriesId) -> Result<Vec<api::Season>>;
async fn upsert_season(&self, ...) -> Result<api::Season>;
async fn replace_seasons(&self, series_id: SeriesId, seasons: &[…]) -> Result<Vec<api::Season>>;
// episodes
async fn episodes(&self, series_id: SeriesId, season: SeasonNumber) -> Result<Vec<api::Episode>>;
async fn episode(&self, id: EpisodeId) -> Result<Option<api::Episode>>;
async fn replace_episodes(&self, series_id: SeriesId, eps: &[…]) -> Result<()>;
// movies
async fn list_movies(&self) -> Result<Vec<api::Movie>>;
async fn movie(&self, id: MovieId) -> Result<Option<api::Movie>>;
async fn upsert_movie(&self, ...) -> Result<api::Movie>;
async fn delete_movie(&self, id: MovieId) -> Result<()>;
// watched
async fn mark_watched(&self, kind: WatchedKind, ts: Timestamp) -> Result<api::Watched>;
async fn remove_watched(&self, id: WatchedId) -> Result<()>;
async fn watched_for(&self, kind: WatchedKind) -> Result<Vec<api::Watched>>;
// computed
async fn pending(&self, today: Date) -> Result<Vec<api::Pending>>;
async fn schedule(&self, today: Date, days: u32) -> Result<Vec<api::ScheduledDay>>;
async fn watch_next(&self, today: Date) -> Result<Vec<api::Pending>>;
// config
async fn config(&self) -> Result<api::Config>;          // loads all keys, defaults missing
async fn set_config(&self, config: &api::Config) -> Result<()>;
async fn get_config(&self, key: &str) -> Result<Option<String>>;     // raw, for api keys at startup
```

`config()` reads each well-known key (`theme`, `tvdb_legacy_apikey`,
`tmdb_api_key`, `schedule_duration_days`, …) from the `config` table and fills
defaults (mirroring ontv's `Config::default`). `set_config()` writes them all.

`db/Cargo.toml`: `api = { path = "../api", features = ["sqll"] }`, `anyhow`,
`rust-embed`, `sqll` (features `["bundled"]`), `tokio` (`["rt", "sync"]`),
`tracing`, and `jiff` (for `Date`/today computation if not delegated to `api`).

---

## 6. Crate: `server`

Port territory's `territory` crate, dropping all auth. `AppState`:

```rust
#[derive(Clone)]
struct AppState {
    db: Database,
    broadcast: broadcast::Sender<api::AppEvent>,
    channels: Channels,
    cache: DiskCache,                 // image cache (port cache.rs)
    http: reqwest::Client,            // for TMDB/TVDB + image fetch
    sync: SyncHandle,                 // tracks in-flight sync tasks (Arc<Mutex<…>>)
}
```

### 6.1 `main.rs`

- `clap` `Args`: `--db-path ontv.db`, `--bind 127.0.0.1:3000`,
  `--cache-dir image-cache`, `--title "ontv"`. (No bootstrap email, no google
  flags, no lantmateriet credentials.)
- Init tracing (copy territory).
- `Database::open(&args.db_path)`.
- `let (broadcast, _) = broadcast::channel(64);`
- Build `reqwest::Client` (rustls).
- Router:
  ```rust
  let app = Router::new()
      .route("/ws", get(ws::ws_handler))
      .route("/api/image/{source}/{*path}", get(proxy::image_handler))
      .route("/api/config", get(http::config));   // title for window
  #[cfg(feature = "bundle")]
  let app = app.fallback(get(static_assets::handler));
  let app = app.layer(CorsLayer::permissive()).with_state(state);
  ```
- Bind + `axum::serve` with `tokio::select!` on ctrl-c (copy territory).

### 6.2 `ws.rs`

`WsHandler { db, broadcast, http, cache, sync }` implementing `ws::Handler`
(`type Id = api::Request; type Response = Result<()>`). The `ws_handler` upgrade
function is territory's minus auth: no `try_auth`, no `UserAccessRevoked`
break-loop — just subscribe to the broadcast, `axum08::server(socket, handler)
.with_channel_allocator(state.channels.clone())`, and the `tokio::select!`
recv/run loop.

`handle` matches every `api::Request` variant. Pattern per the brief:

```rust
api::Request::ListSeries => {
    let _ = incoming.read::<api::ListSeriesRequest>()?;
    let series = self.db.list_series().await?;
    outgoing.write(api::ListSeriesResponse { series });
}
api::Request::MarkWatched => {
    let req = incoming.read::<api::MarkWatchedRequest>()?;
    let ts = req.timestamp.unwrap_or_else(api::Timestamp::now);
    let watched = self.db.mark_watched(req.kind, ts).await?;
    let _ = self.broadcast.send(api::AppEvent {
        channel: incoming.channel(),
        kind: api::AppEventKind::WatchedChanged { kind: req.kind },
    });
    // episode watched flag flipped → also notify detail views, and queues
    let _ = self.broadcast.send(api::AppEvent {
        channel: incoming.channel(),
        kind: api::AppEventKind::PendingChanged,
    });
    outgoing.write(api::MarkWatchedResponse { watched });
}
```

Endpoint → handler responsibilities:

| Request        | DB action                              | Broadcast(s) emitted                         |
| -------------- | -------------------------------------- | -------------------------------------------- |
| ListSeries     | `list_series`                          | —                                            |
| GetSeries      | `series`                               | —                                            |
| ListSeasons    | `seasons`                              | —                                            |
| ListEpisodes   | `episodes`                             | —                                            |
| GetEpisode     | `episode`                              | —                                            |
| ListMovies     | `list_movies`                          | —                                            |
| GetMovie       | `movie`                                | —                                            |
| TrackSeries    | upsert (placeholder) + spawn sync      | `SeriesCreated`, then sync events (§9)        |
| UntrackSeries  | `set_series_tracked(false)`            | `SeriesChanged`                               |
| RemoveSeries   | `delete_series`                        | `SeriesDeleted`, `PendingChanged`             |
| TrackMovie     | upsert + spawn sync                     | `MovieCreated`, sync events                    |
| RemoveMovie    | `delete_movie`                         | `MovieDeleted`, `PendingChanged`              |
| MarkWatched    | `mark_watched`                         | `WatchedChanged`, `PendingChanged`            |
| RemoveWatched  | `remove_watched`                       | `WatchedChanged`, `PendingChanged`            |
| ListWatched    | `watched_for`                          | —                                            |
| ListPending    | `pending(today)`                       | —                                            |
| ListSchedule   | `schedule(today, days)`                | —                                            |
| ListWatchNext  | `watch_next(today)`                    | —                                            |
| Search         | call TMDB/TVDB live (§9)               | —                                            |
| SyncSeries     | spawn background sync task              | `SyncStarted` now; `SeriesChanged`/`SeasonsChanged`/`EpisodesChanged`/`SyncFinished` later |
| SyncAll        | spawn sync for every tracked series    | per-series sync events                        |
| GetConfig      | `config`                               | —                                            |
| SetConfig      | `set_config`                           | `ConfigChanged`                               |
| Unknown(id)    | `bail!`                                 | —                                            |

`today` is computed server-side from `jiff::Zoned::now().date()` →
`api::Date`.

### 6.3 `proxy.rs` — image proxy

Port territory's `tile_handler` shape, but unauthenticated and source/path
based. Route: `/api/image/{source}/{*path}` where `source ∈ {tvdb, tmdb}`.

```rust
async fn image_handler(
    State(state): State<AppState>,
    Path((source, path)): Path<(String, String)>,
) -> Response {
    // validate source
    let upstream = match source.as_str() {
        "tmdb" => format!("https://image.tmdb.org/t/p/original/{path}"),
        "tvdb" => format!("https://artworks.thetvdb.com/{path}"),  // banners/posters host
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let bytes = state.cache.get_or_fetch(&source, &path, async || {
        let resp = state.http.get(&upstream).send().await?;
        if resp.status() == 404 { return Ok(None); }
        Ok(Some(resp.bytes().await?))
    }).await;
    // image/* + Cache-Control: public, max-age=…, immutable
}
```

Reuse `cache.rs` from territory (rename keys to `{source}/{path}`; sanitize
`path` to forbid `..`). Content type via `mime_guess` from the path extension,
defaulting to `image/jpeg`.

### 6.4 `static_assets.rs`

Copy verbatim (embeds `../../dist`, SPA fallback to `index.html`). Behind the
`bundle` feature, on by default, exactly like territory.

### 6.5 `http.rs`

Single handler `config` returning the window title as JSON
(`{ "title": "ontv" }`). Frontend reads it to set `document.title`. Drop
`login`/`logout`/`me`/`register`.

`server/Cargo.toml`: `api`, `db`, `anyhow`, `axum = "0.8.9"`,
`musli-web` (features `["axum08"]`), `clap` (derive+env), `tokio` (full),
`reqwest` (rustls, json), `bytes`, `serde`/`serde_json`, `jiff`,
`tower-http` (cors), `tracing`/`tracing-subscriber`, `mime_guess` + `rust-embed`
(behind `bundle`), `lru`/`parking_lot` (cache), `url`. **No** `bcrypt`, `hmac`,
`sha2`, `cookie`, `axum-extra`, `base64`, `rand` unless something incidentally
needs them.

---

## 7. Crate: `frontend`

Yew 0.23 CSR, `musli-web` `["web03", "yew023"]`. Copy verbatim from territory:
`setup_channel.rs`, `error.rs`, the `ws::Handle` context plumbing, and the
`Root` → `App` shell pattern (minus login/register/admin/auth states — `Root`
goes straight to `App` once `RouterState` + `ws::Handle` exist).

### 7.1 Data-ownership rules (apply to every component)

1. A component that **displays** entity data owns it: store it in component
   state (`Vec<api::Series>`, `Option<api::Series>`, …), never receive it via
   props.
2. On `create`, grab `ws::Handle` from context, attach
   `_broadcast_listener = ws.clone().on_broadcast(link.callback(Msg::AppBroadcast))`,
   and `_setup: SetupChannel::new(ws, link.callback(Msg::Channel))`.
3. On `Msg::Channel(Ok(ch))` with `ch.id() != ChannelId::NONE`, issue the
   component's list/get request(s) via `self.channel.request().body(req)
   .on_packet(link.callback(Msg::Loaded)).send()`, holding the returned
   `ws::Request` in a `_*_req` field.
4. On `Msg::Loaded(res)`, `self.data = res?.decode()?.field;` and re-render.
5. On `Msg::AppBroadcast(packet)`:
   `let event = packet?.decode_event()?;`
   **`if event.channel == self.channel.id() { return Ok(false); }`** (skip own
   echo), then `match event.kind { … }` to patch local state in place
   (`push`/`retain`/find-and-mutate) or re-issue the list request for coarse
   events.
6. **Props carry only**: ids (which entity to show), routing callbacks
   (`on_route_change`), and an `onerror: Callback<Error>`. Never an entity, and
   never a `changed`/`on_change` prop feeding data downward.

Every component wraps `update` in a `try_update` returning `Result<bool, Error>`
and forwards errors to `ctx.props().onerror` (territory pattern).

### 7.2 Router (`router.rs`)

Port territory's history-API `RouterState`. `Route` enum:

```rust
pub enum Route {
    Dashboard,
    Queue,
    WatchNext,
    SeriesList,
    Series(SeriesId),
    Season(SeriesId, SeasonNumber),
    MoviesList,
    Movie(MovieId),
    Search(Option<String>),
    Settings,
}
```

URL mapping (`/`, `/queue`, `/watch-next`, `/series`, `/series/{id}`,
`/series/{id}/season/{n}` where `n = "s"` for specials, `/movies`,
`/movies/{id}`, `/search?q=…`, `/settings`). No `Login`/`Register`/`Admin`.

### 7.3 `app.rs` — shell

Toolbar + body using territory classes. Toolbar nav buttons
(`btn-icon` + `icon <name>`) route to Dashboard, Queue, Watch-next, Series,
Movies, Search, Settings. Body is `<div class="app-body">` hosting the page
component selected by `ctx.props().route`. Pass `onerror` and
`on_route_change` down to pages. `App` owns no entity data itself.

### 7.4 Pages — data loaded & events reacted to

| Page (component)   | Loads on channel-open                                  | Reacts to broadcasts (reload or patch)                                |
| ------------------ | ------------------------------------------------------ | --------------------------------------------------------------------- |
| `Dashboard`        | `ListPending`, `ListSchedule { days }`                 | `PendingChanged`, `WatchedChanged`, `EpisodeChanged`, `SyncFinished` → reload both |
| `Queue`            | `ListPending`                                          | `PendingChanged`, `WatchedChanged` → reload                            |
| `WatchNext`        | `ListWatchNext`                                         | `PendingChanged`, `WatchedChanged` → reload                           |
| `SeriesList`       | `ListSeries`                                            | `SeriesCreated` push, `SeriesChanged` patch, `SeriesDeleted` retain    |
| `Series`           | `GetSeries{id}`, `ListSeasons{id}`                     | `SeriesChanged` (matching id) patch, `SeasonsChanged` replace seasons   |
| `Season`           | `ListEpisodes{series_id, season}`                      | `EpisodeChanged` patch, `EpisodesChanged` (matching) reload, `WatchedChanged` reload |
| `MoviesList`       | `ListMovies`                                           | `MovieCreated`/`MovieChanged`/`MovieDeleted`                           |
| `Movie`            | `GetMovie{id}`, `ListWatched{Movie{id}}`               | `MovieChanged` patch, `WatchedChanged` (matching) reload history       |
| `Search`           | nothing until query submitted; sends `Search`          | `SeriesCreated`/`MovieCreated` → flip "already tracked" markers        |
| `Settings`         | `GetConfig`                                             | `ConfigChanged` patch                                                  |

Each page issues mutations (`MarkWatched`, `RemoveWatched`, `TrackSeries`,
`UntrackSeries`, `RemoveSeries`, `SetConfig`, `SyncSeries`, …) via its channel
and relies on broadcasts (or the direct response) to update. Because the sender
filters its own `channel`, the page updates from its own response and *other*
clients update from the broadcast — no double application.

### 7.5 Reusable data-owning components

- `series_banner.rs` — `Props { series_id, … }`. Owns its `Option<Series>`:
  loads via `GetSeries{id}` on channel-open, listens for `SeriesChanged`. Renders
  poster (via proxy) + title + track/untrack/sync buttons. Used by `Series`,
  `Dashboard`, `Queue` rows. (This is the canonical example of "components own
  their data by querying the backend".)
- `movie_banner.rs` — same for `Option<Movie>` via `GetMovie{id}`.
- `episode_row.rs` — given an `api::Episode` (passed by the owning `Season`
  page, which *does* own the episode list — this is list rendering, not entity
  prop-drilling across a data boundary) renders number/name/air-date + a
  `watch_button`.
- `watch_button.rs` — `Props { kind: WatchedKind, watched: bool }` + callbacks;
  sends `MarkWatched`/`RemoveWatched`. Presentational w.r.t. the flag (the owning
  page reloads on `WatchedChanged`).
- `calendar.rs` — presentational; renders `Vec<ScheduledDay>` passed by
  `Dashboard`.

> The line: **lists** of entities are owned by the page that lists them and
> handed to row components for rendering; a component that represents **one
> entity in isolation** (banner/detail) owns that entity by querying for it.

### 7.6 Images (`image.rs`)

A tiny helper / component turning `api::Image` into `<img src=…>` using
`image.proxy_url()` → `/api/image/{source}/{path}`. Use territory classes for
sizing; add at most one or two image-specific classes if `style/main.scss`
lacks them.

### 7.7 `lib.rs` / `index.html` / `style`

- `lib.rs`: module declarations + `#[wasm_bindgen(start)]` (copy territory's
  tracing-wasm setup), `yew::Renderer::<root::Root>::new().render()`.
- `index.html`: copy territory's; keep the heroicons `copy-dir` line and
  `style/main.scss`; drop any map-only assets.
- `style/`: reuse territory's `main.scss` + `_icons.scss` so the class
  vocabulary and icon set are available unchanged.

`frontend/Cargo.toml`: copy territory's (`api`, `yew = "0.23"`,
`musli-web` `["web03","yew023"]`, `gloo`, `js-sys`, `wasm-bindgen`, `web-sys`
with the DOM features used, `serde`/`serde_json`, `tracing`/`tracing-wasm`,
`url`, `derive_more`). Drop `ResizeObserver`/map web-sys features if unused.

---

## 8. Image handling

- Storage: images are `TEXT` like `tvdb:/posters/abc.jpg` or `tmdb:/xY.jpg`
  (`api::Image`). The frontend never hits TMDB/TVDB directly.
- Frontend renders `<img src="/api/image/{source}/{path}">` from
  `Image::proxy_url()`.
- Server `proxy::image_handler`: validate `source ∈ {tvdb,tmdb}`, reject `..` in
  `path`, build the upstream URL, **cache-through** via `DiskCache`
  (ported from territory `cache.rs`), return bytes with
  `Cache-Control: public, max-age=604800, immutable`. 404 upstream → 404; other
  upstream errors → 502.
- Upstream hosts: TMDB `https://image.tmdb.org/t/p/original/{path}`; TVDB
  `https://artworks.thetvdb.com/{path}` (the sync client stores the
  artwork-relative path so this concatenation is valid).

---

## 9. TMDB / TVDB sync

Port ontv's `src/api/themoviedb.rs` and `src/api/thetvdb.rs` into
`server/src/tmdb.rs` / `tvdb.rs` as `reqwest`-based clients (keep their `serde`
response structs; convert results into `api::*` types — `RemoteId`/`Image`
strings, `Date` from the API's date strings, `SeasonNumber` from numbers). API
keys come from the `config` table (`tmdb_api_key`, `tvdb_legacy_apikey`) loaded
via `db.get_config(...)` at request time.

### Search (`Search` endpoint — synchronous to the caller)

`handle(Search)` calls the relevant client live (`SearchKind::Series` →
series search on TMDB and/or TVDB; `Movies` → movie search), maps hits to
`SearchSeries`/`SearchMovie`, sets `already_tracked` by checking
`series_by_remote`/`movie_by_remote`, and writes `SearchResponse`. No DB writes,
no broadcast.

### Track + initial sync

`TrackSeries{remote_id}`:
1. `upsert_series` a placeholder row (title from search if available, `tracked =
   1`) → broadcast `SeriesCreated` and return the `Series` immediately so the UI
   navigates to the detail page.
2. Spawn a background sync task (below) for that series.

`TrackMovie{remote_id}` analogous (`MovieCreated`, then movie sync — movies are
a single fetch, can also be done inline before responding if cheap).

### Background sync task (`sync.rs`)

`SyncSeries{id}` and `SyncAll` spawn `tokio::task`s. A `SyncHandle`
(`Arc<Mutex<HashSet<SeriesId>>>` in `AppState`) dedupes in-flight syncs so the
same series isn't synced twice concurrently. Each task:

1. broadcast `SyncStarted { series_id: Some(id) }`.
2. Load the series' `remote_id`; fetch full metadata + seasons + episodes from
   the matching remote client.
3. `upsert_series` (metadata) → broadcast `SeriesChanged { series }`.
4. `replace_seasons(series_id, …)` → broadcast `SeasonsChanged { series_id,
   seasons }`.
5. `replace_episodes(series_id, …)` → broadcast `EpisodesChanged { series_id,
   season: … }` (one per affected season, or a single coarse event).
6. broadcast `SyncFinished { series_id: Some(id) }`, and `PendingChanged` (new
   aired episodes may have appeared).
7. Remove the id from `SyncHandle`.

Broadcasts use `channel: ChannelId::NONE` (sync isn't triggered by a single
channel's request semantics — or pass `incoming.channel()` of the triggering
request so that client also refreshes; since the data is server-authored,
`NONE` is correct so *all* clients including the trigger update).

`SyncAll` enumerates `list_series` and spawns/queues per-series syncs (bounded
concurrency via a `tokio::sync::Semaphore`).

> Network/HTTP details (auth headers, pagination, JSON shapes) are lifted
> directly from ontv's existing `themoviedb.rs` / `thetvdb.rs`; only the output
> mapping (to `api::*` instead of ontv's `model`) and the storage path
> (`Database` upserts instead of in-memory maps) change.

---

## 10. Dependency list (`cargo add` commands)

Run from each crate directory (or with `-p <crate>`). Create crates first with
`cargo new --lib crates/api` etc. (and edit the binary crate's `[[bin]]`),
then:

### `api`
```sh
cargo add -p api musli-core@0.1.4 --no-default-features --features alloc,std
cargo add -p api musli-web@0.4.2 --features api
cargo add -p api jiff@0.2
cargo add -p api serde@1 --features derive
cargo add -p api --optional sqll@0.13.2
# then add the feature manually in Cargo.toml:  [features] sqll = ["dep:sqll"]
```

### `db`
```sh
cargo add -p db --path ../api --features sqll          # or edit path dep + features
cargo add -p db anyhow@1
cargo add -p db rust-embed@8.11.0
cargo add -p db sqll@0.13.2 --features bundled
cargo add -p db tokio@1 --features rt,sync
cargo add -p db tracing@0.1
cargo add -p db jiff@0.2
```

### `server`
```sh
cargo add -p server --path ../api
cargo add -p server --path ../db
cargo add -p server anyhow@1
cargo add -p server axum@0.8.9
cargo add -p server musli-web@0.4.2 --no-default-features --features axum08
cargo add -p server clap@4 --features derive,env
cargo add -p server tokio@1 --features full
cargo add -p server reqwest@0.13 --no-default-features --features rustls,json
cargo add -p server bytes@1
cargo add -p server serde@1 --features derive
cargo add -p server serde_json@1
cargo add -p server jiff@0.2
cargo add -p server tower-http@0.6 --features cors
cargo add -p server tracing@0.1
cargo add -p server tracing-subscriber@0.3 --features env-filter
cargo add -p server url@2
cargo add -p server lru@0.18
cargo add -p server parking_lot@0.12
cargo add -p server --optional mime_guess@2 --no-default-features
cargo add -p server --optional rust-embed@8
# [features] default = ["bundle"]; bundle = ["dep:rust-embed", "dep:mime_guess"]
```

### `frontend`
```sh
cargo add -p frontend --path ../api
cargo add -p frontend yew@0.23 --features csr
cargo add -p frontend musli-web@0.4.2 --no-default-features --features web03,yew023
cargo add -p frontend gloo@0.12
cargo add -p frontend js-sys@0.3
cargo add -p frontend wasm-bindgen@0.2
cargo add -p frontend serde@1 --features derive
cargo add -p frontend serde_json@1
cargo add -p frontend tracing@0.1
cargo add -p frontend tracing-wasm@0.2
cargo add -p frontend url@2
cargo add -p frontend derive_more@2 --features display
cargo add -p frontend web-sys@0.3 --features Window,Document,Location,History,HtmlInputElement,HtmlElement,Element,EventTarget,Storage,Url
```

(Set `frontend` `[lib] crate-type = ["cdylib", "rlib"]`.)

---

## 11. Implementation order

1. **Workspace skeleton** — root `Cargo.toml` (`members = ["crates/*"]`),
   `.gitignore`, `Trunk.toml`. Create the four crates.
2. **`api`** — `define_id!`, `Timestamp`, `Date`, `RemoteId`/`Image` string
   newtypes, `SeasonNumber` (+ `to_i64`/`from_i64`), `ThemeType`, all data
   structs, all request/response structs, `AppEvent`/`AppEventKind`, the
   `api::define!` block. `cargo build -p api` and
   `cargo build -p api --features sqll`.
3. **`db`** — migration SQL, copy migration runner + `statements!` macro from
   territory, `Row` structs, `Database` methods (§5). Add a tiny smoke test that
   opens an in-memory/temp db and round-trips a series + episode + watch.
   `cargo build -p db`.
4. **`server`** — `cache.rs`, `proxy.rs`, `static_assets.rs`, `http::config`,
   `main.rs` wiring, then `ws.rs` read-only endpoints first (List*/Get*), verify
   over a manual WS client. Then mutation endpoints + broadcasts. Then
   `tmdb.rs`/`tvdb.rs` + `sync.rs` + `Search`/`Track*`/`Sync*`.
   `cargo build -p server`.
5. **`frontend`** — copy `setup_channel.rs`, `error.rs`, `ui.rs`, `style/`,
   `index.html`; build `root.rs`/`app.rs`/`router.rs`. Then pages in dependency
   order: `SeriesList` → `Series` → `Season` → `MoviesList` → `Movie` →
   `Dashboard`/`Queue`/`WatchNext` → `Search` → `Settings`. Add
   `series_banner`/`movie_banner`/`episode_row`/`watch_button`/`calendar`/`image`
   as needed. Build with `trunk build`.
6. **End-to-end**: `trunk build` → run `server` with `--bundle` default →
   open browser → track a series → confirm sync events flow and a second tab
   updates live via broadcasts.
7. **Polish**: settings persistence, schedule calendar, watch-history views.

Commit at the end of each crate milestone (on a feature branch, per repo
conventions; do not push unless asked).

---

## 12. Key design decisions / gotchas

- **IDs UUID → u64.** All ids become SQLite `INTEGER PRIMARY KEY` rowids wrapped
  by `define_id!` (`i64.cast_unsigned()` ↔ `u64`). The old UUIDs are not
  migrated (this is a fresh DB). `remote_id` (TEXT, unique) is the stable
  cross-source key used by sync to upsert.
- **Seasons get a surrogate PK.** ontv seasons had no id; we add
  `seasons.id INTEGER PRIMARY KEY` + `SeasonId` and a `UNIQUE(series_id,
  number)` constraint so upserts are idempotent.
- **`SeasonNumber::Specials` stored as `0`.** Regular seasons are ≥ 1 in
  TMDB/TVDB, so `0` is unambiguous. Map via `to_i64`/`from_i64`; never store the
  enum directly. (ontv's IMDB url logic used `-1` for specials — that's a URL
  concern, not storage; keep storage at `0`.)
- **RemoteId / Image / Date are TEXT string types**, not musli enums — encode
  the ontv `Display` format (`tvdb:123`, `tmdb:/path.jpg`, `YYYY-MM-DD`). Keeps
  SQLite columns plain and the wire format compact.
- **Times = `jiff::Timestamp` as TEXT** (RFC-3339), via territory's `Timestamp`
  newtype. Air/release dates are date-only `Date` TEXT, no time component.
- **No `pending` table.** Pending/queue/schedule/watch-next are computed SQL
  queries; the server recomputes on demand and the UI reloads them on
  `PendingChanged`/`WatchedChanged`.
- **Images via proxy** at `/api/image/{source}/{path}` (`source = tvdb|tmdb`),
  cache-through on disk. Validate source, forbid `..` in path.
- **No auth.** Single-user local app. Drop sessions, login/logout/register,
  google-oauth, bootstrap admin, `auth.rs`/`session.rs`/`google_oauth.rs`, the
  `users`/`sessions`/`registration_tokens` tables, and the `UserAccessRevoked`
  WS break-loop. The WS upgrade has no auth gate. **No login/registration
  pages.**
- **Config in a key/value table**, loaded at startup and on `GetConfig`,
  defaults filled to match ontv's `Config::default`. API keys for TMDB/TVDB live
  here (set via the Settings page) and are read by the sync clients per request.
- **Sync = tokio background tasks** triggered by `SyncSeries`/`SyncAll`/
  `TrackSeries`; deduped via a `SyncHandle` set; progress streamed to all
  clients through broadcasts (`SyncStarted`/`SeriesChanged`/`SeasonsChanged`/
  `EpisodesChanged`/`SyncFinished` + `PendingChanged`). Search is synchronous to
  the requesting channel.
- **Broadcast echo-suppression.** Every mutation broadcasts with
  `channel: incoming.channel()`; every listener does
  `if event.channel == self.channel.id() { return Ok(false); }` so the
  initiating client updates from its direct response, not the echo. Server-
  authored events (sync) use `ChannelId::NONE` so *all* clients update.
- **Data ownership over prop-drilling.** Components that show a single entity
  query for it (`series_banner` → `GetSeries`); pages own the lists they render
  and pass individual items to row components for rendering only. **No
  `fn changed`/`on_change` props carrying entity data downward.** Props =
  ids + routing + `onerror` only.
- **Reuse territory's CSS classes** (`app`, `app-body`, `toolbar`, `row`,
  `row-fill`, `section`, `fill`, `btn`, `btn-icon`, `btn-icon-success`,
  `btn-icon-danger`, `input-group`, `input-text`, `title`, `empty`,
  `section-header`, `icon <name>`, `icon-inline`) and `style/main.scss`
  verbatim. Add new classes only when a layout is genuinely unexpressible with
  the existing set.
```