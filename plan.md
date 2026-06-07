# ontv-musli-web — Implementation Plan

A Yew/Rust web port of the **ontv** desktop TV/movie tracker, structured after
the **territory** template (`~/repo/territory`). Single-user, no auth. SQLite via
`sqll`, `musli-web` WebSocket RPC, a global broadcast event stream, data-owning
components, and a server-side image proxy.

This document tracks **what is done** and **what remains**. The initial
crate-by-crate blueprint is complete and is not repeated here — read the source.

---

## Current state (compiles cleanly, all four crates)

### `crates/api`
All shared types, request/response structs, the `api::define!` endpoint block,
and broadcasts. Notable:
- `SyncMovieRequest`/`SyncMovie`, `UntrackSeriesRequest { id, tracked }` (toggle),
  `RemoveWatchedRequest { id, kind }` (kind carried for echo-suppressed
  broadcasts).
- Task queue types: `TaskId`, `Task`, `CompletedTask`, `TaskKind`
  (`SyncSeries`/`SyncMovie`), `TaskStatus`, `ListTasksRequest`/`ListTasks`.
- `Episode` carries `watched`, `watched_count`, and `last_watched_id:
  Option<WatchedId>`.
- `AppEventKind`: `SeriesCreated/Changed/Deleted`, `SeasonsChanged`,
  `EpisodeChanged`, `EpisodesChanged`, `MovieCreated/Changed/Deleted`,
  `WatchedChanged`, `PendingChanged`, `ConfigChanged`, and
  **`TaskAdded`/`TaskStarted`/`TaskCompleted`**. There is **no**
  `SyncStarted`/`SyncFinished` — sync progress is surfaced via the task events.
- `Config { theme, tvdb_legacy_apikey, tmdb_api_key, schedule_duration_days,
  dashboard_limit, dashboard_page }`. (No `schedule_limit`/`schedule_page`, no
  `auto_sync_*` yet — see Remaining work.)

### `crates/db`
Full schema, migrations, `statements!` block, async methods, computed
pending/schedule/watch-next queries, key/value config table. Done.

### `crates/server`
- `ws.rs` — all handlers: ListSeries, GetSeries, ListSeasons, ListEpisodes,
  TrackSeries, UntrackSeries, RemoveSeries, SyncSeries, ListMovies, GetMovie,
  TrackMovie, RemoveMovie, SyncMovie, MarkWatched, RemoveWatched, ListWatched,
  ListPending, ListSchedule, ListWatchNext, Search, SyncAll, GetConfig,
  SetConfig, ListTasks.
- `sync.rs` — `sync_series` / `sync_movie` (TMDB + TVDB), broadcasting
  `SeriesChanged`/`SeasonsChanged`/`EpisodesChanged`/`PendingChanged` with
  `channel: ChannelId::NONE`.
- Broadcast plumbing now flows through `app_broadcast::Broadcaster` (owned,
  cloneable wrapper) passed across `main`/`ws`/`task_queue`/`sync` instead of
  passing raw `tokio::sync::broadcast::Sender` values around.
- `task_queue.rs` — `TaskQueue` with pending (delayed)/running/completed
  tracking, dedup by series/movie id, and a `run(db, remote, broadcast)` worker
  loop spawned from `main`. `push(kind, immediate, &broadcast)` enqueues and
  broadcasts `TaskAdded`.
- `tmdb.rs`, `tvdb.rs`, `remote.rs` — remote clients + search.
- `cache.rs`, `proxy.rs`, `static_assets.rs` — disk image cache, `/api/image/
  {source}/{*path}` proxy, SPA serving.
- `main.rs` — `Args { db_path, cache_dir, bind }`, builds `AppState { db,
  broadcast, channels, http, cache, queue, remote }`, spawns the queue worker.

### `crates/frontend`
- `app.rs` — toolbar (`toolbar` / `toolbar-inner row-fill`, nav constrained to
  960px), hosts the routed page.
- `dashboard.rs` — pending grid + schedule; reacts to `PendingChanged`,
  `WatchedChanged`, `SeriesCreated/Deleted`, `MovieCreated/Deleted`.
- `series.rs` — `SeriesList`, filter + pagination (`PAGE_SIZE = 20`),
  `table`/`table-entry` layout.
- `series_detail.rs` — seasons sidebar + episode list; mark-watched,
  watch-remaining, remove-last-watch, track/untrack toggle, sync, remove with
  **inline `confirm_remove: bool`**.
- `movies.rs` — `MoviesList`, filter + pagination, mark-watched from list.
- `movie_detail.rs` — poster, overview, watch history section, mark/remove
  watched, sync (when `remote_id` present), remove with **inline
  `confirm_remove: bool`**.
- `queue.rs` — live task queue (pending/running/completed), sync-all button,
  pagination on pending.
- `search.rs` — live TMDB/TVDB search, track series/movie.
- `watch_next.rs` — watch-next list with mark-watched.
- `settings.rs` — Config form (theme, API keys, dashboard limit / page,
  schedule duration).
- CSS conventions: pages own padding via `.page` / `.page-title` (the old
  `outline`/`outline-title` names are **not** used). `table`/`table-entry`,
  `input-group`, `toolbar-inner`, `detail-layout`/`detail-sidebar`/
  `detail-content`, `actions`, `group`, `empty` are all in use.
- There is **no `ui.rs` module** yet — no shared `ConfirmDanger`/`ErrorBanner`.

### CSS rule (still binding)
Reuse the existing territory class vocabulary from
`crates/frontend/style/main.scss`. Do not invent new class sets; add at most one
or two classes only when a layout is genuinely unexpressible.

---

## Remaining work

### 1. Shared `ConfirmDanger` component

Destructive actions currently use ad-hoc inline `confirm_remove: bool` state
(`series_detail.rs`, `movie_detail.rs`). Replace these with a shared component
ported from territory.

**Component (port from `~/repo/territory/crates/frontend/src/ui.rs`):**

```rust
#[derive(Properties, PartialEq)]
pub(super) struct ConfirmDangerProps {
    pub(super) label: AttrValue,        // the thing being acted on (bolded)
    pub(super) prompt: AttrValue,       // e.g. "Are you sure you want to remove"
    pub(super) on_confirm: Callback<()>,
    pub(super) on_cancel: Callback<()>,
    #[prop_or_default]
    pub(super) btn_class: Classes,
}

#[function_component]
pub(super) fn ConfirmDanger(props: &ConfirmDangerProps) -> Html { … }
```

It renders a `row-fill` with `"<prompt> <strong>label</strong>?"` and an
`input-group end` containing a cancel `btn-icon` (`icon x-mark`) and a confirm
`btn-icon-danger` (`icon check`); both callbacks `stop_propagation`. All classes
already exist — no new CSS.

**Steps:**
- Create `crates/frontend/src/ui.rs` with `ConfirmDanger` (copy territory's
  verbatim). Add `mod ui;` to `lib.rs` and `use self::ui::ConfirmDanger;` where
  needed. (Optionally also port `ErrorBanner` while here, but that is out of
  scope for this task unless wanted.)
- **`series_detail.rs`**: the remove control in `view_header` currently swaps in
  inline "Confirm remove"/"Cancel" buttons driven by `confirm_remove`. Keep the
  `confirm_remove` field as the toggle, but when set, render `<ConfirmDanger
  prompt="Remove series" label={series.title.clone()} on_confirm={…
  Msg::RemoveSeries} on_cancel={… Msg::CancelRemove} />` instead of the two raw
  buttons. The `ConfirmRemove`/`CancelRemove`/`RemoveSeries` messages stay.
- **`movie_detail.rs`**: same treatment in its `view_header` (`prompt="Remove
  movie" label={movie.title.clone()}`).
- The per-episode and per-movie **watch-removal** controls are not "danger"
  confirmations and can be left as-is (single-click remove of a watch entry).

### 2. Automatic background sync

Add an opt-in periodic task that re-syncs every tracked series and movie.

**`api` (`Config`):** add two fields (update `Default`, the `Encode`/`Decode`
derive handles the wire):
```rust
pub auto_sync_enabled: bool,        // default false
pub auto_sync_interval_hours: u32,  // default 24, min clamp 1 server-side
```

**`db`:** persist the two new keys in `load_config`/`save_config` (the config
table is key/value; add `auto_sync_enabled` and `auto_sync_interval_hours` to
the read/write set, defaulting like `Config::default`).

**`server`:** spawn a background loop in `main.rs` next to the queue worker. It
owns clones of `db`, `queue`, and `broadcast` and re-reads config each tick so a
settings change takes effect without restart:

```rust
tokio::spawn({
    let db = db.clone();
    let queue = queue.clone();
    let broadcast = broadcast.clone();
    async move {
        loop {
            let config = db.load_config().await.unwrap_or_default();
            let hours = config.auto_sync_interval_hours.max(1) as u64;
            tokio::time::sleep(Duration::from_secs(hours * 3600)).await;

            let config = db.load_config().await.unwrap_or_default();
            if !config.auto_sync_enabled { continue; }

            for s in db.series().await.unwrap_or_default() {
                queue.push(api::TaskKind::SyncSeries { series_id: s.id, title: s.title },
                           false, &broadcast).await;
            }
            for m in db.movies().await.unwrap_or_default() {
                queue.push(api::TaskKind::SyncMovie { movie_id: m.id, title: m.title },
                           false, &broadcast).await;
            }
        }
    }
});
```

Reuse the existing `TaskQueue` (its dedup prevents piling duplicate syncs, its
`TASK_DELAY` spacing avoids hammering the remotes). Do **not** invent a parallel
sync path. `Duration` is already imported in `task_queue.rs`; import it in
`main.rs` if needed.

**`frontend` (`settings.rs`):** add a settings section with a checkbox
(`input-checkbox` if present, else a plain `<input type="checkbox">`) bound to
`auto_sync_enabled` and an `input-number` (`field` layout, like the existing
dashboard-limit field) bound to `auto_sync_interval_hours`. Wire both into the
existing `SetConfig` save path.

### 3. Banner graphic on detail pages

`api::Series` and `api::Movie` both already carry `banner: Option<Image>`. It is
never rendered. Show the wide banner at the top of each detail page's content.

- **`series_detail.rs`**: in `view`, between `view_header` and the
  `detail-layout`, render the banner when present:
  ```rust
  if let Some(banner) = self.series.as_ref().and_then(|s| s.banner.as_ref()) {
      html! { <img class="banner" src={banner.proxy_url()} /> }
  }
  ```
- **`movie_detail.rs`**: same, placed above `detail-layout` in `view_body` (or
  in `view` before it), guarded on `movie.banner`.
- CSS: reuse an existing wide-image class if one exists in `main.scss`
  (`poster` is poster-shaped, so not it). If none fits, add a **single**
  minimal `.banner` rule (`width: 100%; max-height: …; object-fit: cover;
  border-radius: …`) — this is the allowed "one or two classes" exception. Check
  `main.scss` first before adding.

### 4. Watch history in series detail

`movie_detail.rs` shows a "Watch history" section (a `section` with a
`group text-muted` header and one `group text-muted` row per `Watched`
timestamp, loaded via `ListWatched`). `series_detail.rs` shows only the
per-episode `watched` flag and `watched_count`, with no timestamps.

Add per-episode history. The cheapest path that matches the existing data flow:
- The episode list already exposes `watched_count` and `last_watched_id`. To
  show full timestamps, load history for the relevant entity via the existing
  `ListWatched { kind: WatchedKind::Episode { series, episode } }` endpoint.
- Recommended scope: when an episode is watched and the user expands it (or
  always, for watched episodes in the selected season), render a small history
  block under the episode `row`, styled like movie_detail's: a `section`
  containing `group text-muted` rows of `w.timestamp.to_string()`. Reuse the
  `last_watched_id` already present for the single-click remove; per-entry
  remove can map each `Watched` to a `RemoveWatched { id, kind }` button.
- Keep it simple and consistent with movie_detail: same classes (`section`,
  `group`, `text-muted`), no new CSS. Loading every episode's history eagerly is
  wasteful — prefer lazy load on expand, or load history for the whole selected
  season once and group by episode id.

### 5. Dashboard reaction to sync/task completion

`dashboard.rs` reloads pending + schedule on `PendingChanged`, `WatchedChanged`,
and the create/delete events, but ignores task events. After a sync completes,
new aired episodes appear in the DB; `sync_series` does broadcast
`PendingChanged` at the end, so the dashboard *does* refresh on a completed
series sync — but only via that one coarse event. To be robust (and to refresh
when a sync finishes for any reason), also react to `TaskCompleted`.

In `dashboard.rs`'s `Msg::AppBroadcast` match, add `TaskCompleted` to the arm
that reloads:
```rust
api::AppEventKind::PendingChanged
| api::AppEventKind::WatchedChanged { .. }
| api::AppEventKind::SeriesCreated { .. }
| api::AppEventKind::SeriesDeleted { .. }
| api::AppEventKind::MovieCreated { .. }
| api::AppEventKind::MovieDeleted { .. }
| api::AppEventKind::TaskCompleted { .. } => { /* reload pending + schedule */ }
```
There is no `SyncFinished` event — use `TaskCompleted`. `TaskAdded`/`TaskStarted`
need no dashboard reaction (no data has changed yet).

---

## Notes / gotchas (still in force)

- **Echo suppression**: every listener does
  `if event.channel == self.channel.id() { return Ok(false); }` before matching.
  Server-authored events (sync, task queue) use `ChannelId::NONE`, so they reach
  all clients including the initiator.
- **Data ownership**: detail components query for their single entity; pages own
  the lists they render. Props carry ids, routing callbacks, and `onerror` only.
- **CSS names**: `page`/`page-title` (not `outline`/`outline-title`). Reuse the
  existing class vocabulary; new classes only as a last resort (banner is the
  one likely exception).
- **No auth.** Single-user local app.
