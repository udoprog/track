# Implementation Plan: Language Selection Feature

## Overview and key design decisions

Three decisions up front, each justified briefly:

1. **Popup rendering: reuse the existing `modal-background` + `modal` pattern, not a CSS dropdown.** The codebase already has a mobile-safe, viewport-bounded, centered overlay used by `ImageGallery` (`width: min(600px, calc(100vw - …))`, `max-height: 80vh`, scrollable). Reusing it means zero new CSS, automatic mobile safety, and built-in click-outside-to-close. This satisfies the CLAUDE.md "no new CSS classes" rule. The picker is a small modal opened by a trigger button.

2. **`LanguagePicker` lives in `crates/frontend/src/ui.rs`** as a `struct` component (it needs internal `open`/`filter`/`page` state), alongside the existing shared components.

3. **Language flows through sync as an `Option<&str>` effective code computed in `sync.rs`**, resolved per call as: per-item override → global config default → `None`. The TMDB client's `get_json` gains an optional language argument; TVDB's fetch methods likewise. The picker stores/emits the ISO 639-1 `part1` code (2-letter, e.g. "en", "de").

---

## 1. Schema migration

Edit the single migration file `crates/db/migrations/2026-06-05.sql` in place (greenfield — merge all changes into one clean file):

- `series` table: add `language TEXT,` (nullable, after `sync_source`, before `last_synced_at`)
- `movies` table: add `language TEXT,` (nullable, after `sync_source`, before `last_synced_at`)
- `config` table: no schema change — `language` is just another key/value row

---

## 2. API structs (`crates/api/src/lib.rs`)

**`Config`** (~line 1196): add field
```rust
pub language: Option<String>,
```
and in `Default` (~line 1209) add `language: None`.

**`Series`** (~line 982): add `pub language: Option<String>,` after `sync_source`.

**`Movie`** (~line 1059): add `pub language: Option<String>,` after `sync_source`.

**New request types** (after `SetMovieSyncSourceRequest`, ~line 1498):
```rust
#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetSeriesLanguageRequest {
    pub id: SeriesId,
    pub language: Option<String>,
}

#[derive(Debug, Encode, Decode)]
#[musli(crate = musli_core)]
pub struct SetMovieLanguageRequest {
    pub id: MovieId,
    pub language: Option<String>,
}
```

**Endpoint registrations** in the `define!` block (after `SetMovieSyncSource`, ~line 1754):
```rust
pub type SetSeriesLanguage;
impl Endpoint for SetSeriesLanguage {
    impl Request for SetSeriesLanguageRequest;
    type Response<'de> = Empty;
}
pub type SetMovieLanguage;
impl Endpoint for SetMovieLanguage {
    impl Request for SetMovieLanguageRequest;
    type Response<'de> = Empty;
}
```

No new `AppEventKind` needed — reuse `SeriesChanged`/`MovieChanged`/`ConfigChanged`.

---

## 3. DB layer (`crates/db/src/lib.rs`)

**Row structs**: add `language: Option<String>` as the **last field** in `SeriesRow` (~line 32) and `MovieRow` (~line 91). Position matters — `#[derive(Row)]` maps by column position.

**Row converters**: `series_from_row` (~line 1902) add `language: r.language`; `movie_from_row` (~line 1958) same.

**Every `SELECT`/`RETURNING` for series and movies must add the column** at the same ordinal position (last). Affected statements:
- Series: `insert_series` (RETURNING), `list_series`, `series_by_id`, `series_by_remote`, `series_needing_sync` — add `s.language` / `language` to each.
- Movies: `insert_movie` (RETURNING), `list_movies`, `movie_by_id`, `movie_by_remote`, `movies_needing_sync` — add `m.language`.

`insert_series` / `insert_movie` do not insert `language` (defaults to NULL) but their `RETURNING` must list it.

**New SQL statements** (near `set_series_sync_source` / `set_movie_sync_source`, ~line 250):
```rust
set_series_language: r#"UPDATE series SET language = ? WHERE id = ?"#,
set_movie_language:  r#"UPDATE movies SET language = ? WHERE id = ?"#,
```

**New async methods** (mirror `set_series_sync_source`, ~line 769). `bind` resets internally — no manual `reset()`:
```rust
pub async fn set_series_language(&self, id: SeriesId, language: Option<String>) -> Result<()> {
    let mut s = self.inner.clone().lock_owned().await;
    spawn_blocking(move || {
        s.set_series_language.bind((language.as_deref(), id))?;
        ensure!(s.set_series_language.step()?.is_done(), "set_series_language");
        Ok(())
    }).await?
}
// analogous set_movie_language
```

**`load_config`** (~line 1839, after `timezone`): add
```rust
let language = self.get_config("language").await?.filter(|v| !v.is_empty());
```
and include `language` in the returned `Config { ... }`.

**`save_config`** (~line 1884, after `timezone`): add
```rust
self.set_config("language", config.language.as_deref().unwrap_or("")).await?;
```

---

## 4. Server sync — language threading (`crates/server/src/`)

Effective language resolution: **per-item override → global config default → `None` (API decides)**. Resolved once in `sync_series` / `sync_movie` in `sync.rs`:

```rust
let config = db.load_config().await?;
let language = series.language.clone().or_else(|| config.language.clone());
// pass language.as_deref() into sync_series_tmdb/tvdb
```

**`tmdb.rs`**: change `get_json` to accept `language: Option<&str>` and append `?language=` when set. Thread into `fetch_series`, `fetch_season_episodes`, `fetch_movie`. (`fetch_movie_releases` and search methods unchanged — release dates / search are language-independent.)

**`tvdb.rs`**: add `language: Option<&str>` to `fetch_series` and `fetch_episodes`; attach `.header("Accept-Language", lang)` when set.

**`remote.rs`**: thread `language: Option<&str>` through all wrapper methods (`fetch_tmdb_series`, `fetch_tmdb_season_episodes`, `fetch_tmdb_movie`, `fetch_tvdb_series`, `fetch_tvdb_episodes`). Language is a per-call arg, not stored on the client struct.

**`sync.rs`**: add `language: Option<&str>` param to `sync_series_tmdb` and `sync_series_tvdb` helper fns; forward to `remote.fetch_*` calls.

---

## 5. WebSocket handlers (`crates/server/src/ws.rs`)

Two new arms mirroring `SetSeriesSyncSource` / `SetMovieSyncSource` but with no remote-source validation:

```rust
api::Request::SetSeriesLanguage => {
    let req = incoming.read::<api::SetSeriesLanguageRequest>().context("missing request")?;
    self.db.set_series_language(req.id, req.language).await?;
    let series = self.db.series_by_id(req.id).await?.context("series not found")?;
    self.broadcast.emit(incoming.channel(),
        api::AppEventKind::SeriesChanged { series: series.clone() },
        "ws set series language changed");
    self.enqueue_series_sync(series.id, series.title, true).await;
    outgoing.write(api::Empty);
}
// analogous SetMovieLanguage arm
```

Re-syncing fetches titles/overviews in the new language. The existing `TaskAdded`/`TaskCompleted` broadcast machinery on the client side already handles spinner + data reload — no new client reaction code needed.

---

## 6. Frontend: `LanguagePicker` component (`crates/frontend/src/ui.rs`)

**Dependency**: add `iso639 = { path = "../iso639" }` to `crates/frontend/Cargo.toml`.

**Props**:
```rust
#[derive(Properties, PartialEq)]
pub(super) struct LanguagePickerProps {
    pub(super) current: Option<String>,        // ISO 639-1 code, or None
    pub(super) on_change: Callback<Option<String>>,
    pub(super) placeholder: &'static str,      // e.g. "Default" or "Override"
}
```

**Struct fields**:
```rust
pub(super) struct LanguagePicker {
    languages: iso639::Languages,   // created once in create()
    open: bool,
    filter: String,
    page: usize,
}
```

**Messages**:
```rust
pub(super) enum LpMsg {
    Open,
    Close,
    Filter(String),
    Page(usize),
    Pick(Option<String>),   // None = clear/use default
}
```

**Constant**: `const PAGE_SIZE: usize = 5;`

**Render breakdown** (zero new CSS classes):

- **Trigger button** (always rendered): a `btn` showing the current selection's `ref_name` via `self.languages.get_by_part1(code)`, or `props.placeholder` if `None`. Example:
  ```html
  <button class="btn" onclick=Open title="Select language">
      <span class="icon-inline"><span class="icon language" /></span>
      <span class="hide-mobile">{label}</span>
  </button>
  ```
  (If no `language` icon exists in the icon set, drop the icon — text-only `btn` is fine.)

- **Popup** (when `self.open`): reuse the `modal-background` / `modal` pattern from `ImageGallery`:
  ```html
  <div class="modal-background" onclick=Close>
      <div class="modal" onclick=stop_propagation>
          <div class="row">
              <span class="fill">{"Language"}</span>
              <button class="btn-icon" onclick=Close><span class="icon x-mark" /></button>
          </div>
          <input class="input-text" placeholder="Filter…" value={filter} oninput=Filter />
          /* "None / Use default" row */
          <div class="table-entry row clickable" onclick=Pick(None)>
              <span class="fill">{props.placeholder}</span>
              if current.is_none() { <span class="icon check" /> }
          </div>
          /* Results */
          for (part1, entry) in filtered.iter().skip(page*5).take(5) {
              <div class="table-entry row clickable" class:active={current==Some(part1)}
                   onclick=Pick(Some(part1))>
                  <span class="fill">{entry.ref_name}</span>
                  <span class="text-muted">{part1}</span>
              </div>
          }
          /* Pagination */
          <PaginationButtons page={page} total_pages={...} on_page=Page />
      </div>
  </div>
  ```

**Update logic**: `Open` → `open=true`, reset `filter`/`page`; `Filter(s)` → set filter, reset `page=0`; `Page(p)` → set page; `Pick(v)` → `props.on_change.emit(v)`, `open=false`; `Close` → `open=false`.

**Filtering**: `self.languages.iter()` filtered by `entry.ref_name.to_lowercase().contains(&filter.to_lowercase())`, collected to a `Vec`, then sliced per page. Case-insensitive contains match.

---

## 7. Wiring into surfaces

**Settings (`crates/frontend/src/settings.rs`)** — global default:
- Add a `LanguageChanged(Option<String>)` message: sets `self.config.language = v`.
- Add to the form (near timezone field):
  ```html
  <div class="field">
      <label>{"Default language"}</label>
      <LanguagePicker current={self.config.language.clone()} placeholder="API default"
          on_change={link.callback(Msg::LanguageChanged)} />
  </div>
  ```
- Saved via the existing Save button → `SetConfig` (no extra request needed; `language` rides in `Config`).

**Series detail (`crates/frontend/src/series_detail.rs`)** — per-series override:
- Add to `use crate::ui::{…}` import.
- New messages: `SetLanguage(Option<String>)`, `SetLanguageDone(Option<String>, Result<…>)`.
- New field: `_set_language_req: ws::Request`.
- `SetLanguage(lang)` handler: send `SetSeriesLanguageRequest { id, language: lang.clone() }`.
- `SetLanguageDone` handler: on Ok, `self.series.as_mut().map(|s| s.language = lang)`.
- Add `SettingLanguage` variant to `crates/frontend/src/error.rs` `Message` enum.
- In the `row fill start` actions block (alongside `RemoteSourceSelect`):
  ```html
  <LanguagePicker current={series.language.clone()} placeholder="Default"
      on_change={link.callback(Msg::SetLanguage)} />
  ```

**Movie detail (`crates/frontend/src/movie_detail.rs`)** — same pattern:
- Same messages/field but using `SetMovieLanguageRequest`.
- Place `LanguagePicker` in the `row fill start` actions block alongside `RemoteSourceSelect`.

---

## 8. Broadcast / reload after language change

No new client reaction code needed — the existing machinery handles everything:

- Server immediately emits `SeriesChanged`/`MovieChanged` (with originating channel → client suppresses echo via `event.channel == self.channel.id()` check).
- Server enqueues immediate sync → `TaskAdded` broadcast (NONE channel) → client sets `syncing=true`.
- Sync completes → `SeriesChanged`/`MovieChanged`/`EpisodesChanged`/`PendingChanged` (NONE channel) → client updates all data.
- `TaskCompleted` → client's existing handler clears spinner and reloads series/seasons/episodes.

Settings: `SetConfig` broadcasts `ConfigChanged` → `settings.rs` already updates `self.config`.

---

## 9. Implementation order (keeps things compiling at each step)

1. **API** (`api/src/lib.rs`): add `language` fields to `Config`/`Series`/`Movie`, new request structs, new endpoint definitions.
2. **DB** (`db/src/lib.rs` + migration): add columns; update `SeriesRow`/`MovieRow` + all SELECT/RETURNING; add statements + methods; update `load_config`/`save_config`.
3. **Server remote/sync** (bottom-up): `tmdb.rs` → `tvdb.rs` → `remote.rs` → `sync.rs`.
4. **Server ws** (`ws.rs`): add `SetSeriesLanguage`/`SetMovieLanguage` arms.
5. **Frontend dep**: add `iso639` to `crates/frontend/Cargo.toml`.
6. **Frontend `LanguagePicker`** (`ui.rs`): implement the struct component; add `SettingLanguage` to `error.rs`.
7. **Wire surfaces**: `settings.rs` → `series_detail.rs` → `movie_detail.rs`.

---

## Key gotchas

- **`#[derive(Row)]` maps by column position** — add `language` as the last field in `SeriesRow`/`MovieRow` and as the last column in every SELECT/RETURNING to avoid mismatches.
- **`bind(...)` resets internally** — do not call `reset()` after bind in the new methods.
- **Empty string vs None in config**: store `None` as `""`, convert back with `.filter(|v| !v.is_empty())`.
- **Modal-background approach**: the picker is a full overlay, not an inline dropdown. This is intentional for mobile safety — the `modal-background` + `modal` pattern is already viewport-bounded.
- **Language code storage**: always the ISO 639-1 two-letter `part1` code (e.g. `"en"`, `"de"`). The `iso639::Languages::iter()` only yields entries that have a `part1` code, so the picker naturally shows only languages with 2-letter codes.
