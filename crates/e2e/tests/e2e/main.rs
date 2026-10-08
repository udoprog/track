//! Browser tests for track. Every test gets a server of its own, with a fresh
//! database and image cache in a temporary directory, serving the frontend that
//! yew-e2e builds for the run. A fresh database has no API keys, so nothing is
//! synced from the remotes. The browser is signed in as the administrator
//! `root` unless a test asks for `(signed_out)`.
//!
//! ```text
//! cargo test -p e2e
//! cargo test -p e2e -- settings::
//! cargo test -p e2e -- --headed --last-session
//! ```

mod auth;
mod dashboard;
mod help;
mod media;
mod movie;
mod navigation;
mod people;
mod queue;
mod search;
mod settings;
mod show;
mod sync;
mod users;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::OnceLock;

use anyhow::{Context, Result, ensure};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use yew_e2e::{Config, Fixture, TestDriver};

/// Media for the tests that ask for it with `(seeded)`.
const SEED: &str = include_str!("seed.sql");
/// More shows on top of [`SEED`] for the tests that ask with `(crowded)`.
const CROWDED: &str = include_str!("crowded.sql");
/// A movie on top of [`SEED`] for the tests that ask with `(movie)`.
const MOVIE: &str = include_str!("movie.sql");
/// Specials and a second season for the seeded show, with `(seasons)`.
const SEASONS: &str = include_str!("seasons.sql");
/// A show airing tomorrow, with `(upcoming)`.
const UPCOMING: &str = include_str!("upcoming.sql");
/// A show nobody tracks, with `(untracked)`.
const UNTRACKED: &str = include_str!("untracked.sql");
/// Poster candidates for the seeded show, with `(graphics)`.
const GRAPHICS: &str = include_str!("graphics.sql");
/// A backdrop for the seeded show, with `(backdrop)`.
const BACKDROP: &str = include_str!("backdrop.sql");
/// A backdrop for the movie in [`MOVIE`], with `(movie, movie_backdrop)`.
const MOVIE_BACKDROP: &str = include_str!("movie_backdrop.sql");
/// A finished show, one with only a special left and one being rewatched,
/// with `(next)`.
const NEXT: &str = include_str!("next.sql");
/// XEM numbering for the seeded show that does not line up, with `(numbering)`.
const NUMBERING: &str = include_str!("numbering.sql");
/// A TMDB remote for every show, with `(remotes)`.
const REMOTES: &str = include_str!("remotes.sql");

/// What a test asks of its server.
#[derive(Default)]
struct Setup {
    /// Start with the library in `seed.sql`.
    seeded: bool,
    /// Start with `seed.sql` and the shows in `crowded.sql`.
    crowded: bool,
    /// Start with `seed.sql` and the movie in `movie.sql`.
    movie: bool,
    /// Start with `seed.sql` and the extra seasons in `seasons.sql`.
    seasons: bool,
    /// Start with `seed.sql` and the show airing tomorrow in `upcoming.sql`.
    upcoming: bool,
    /// Start with `seed.sql` and the untracked show in `untracked.sql`.
    untracked: bool,
    /// Start with `seed.sql` and the poster candidates in `graphics.sql`.
    graphics: bool,
    /// Start with `seed.sql` and the backdrop in `backdrop.sql`.
    backdrop: bool,
    /// Add the backdrop in `movie_backdrop.sql` to the movie from `(movie)`.
    movie_backdrop: bool,
    /// Start with `seed.sql` and the shows in `next.sql`.
    next: bool,
    /// Start with `seed.sql` and the XEM numbering in `numbering.sql`.
    numbering: bool,
    /// Start with `seed.sql` and a TMDB remote for every show, added last.
    remotes: bool,
    /// Start at the sign-in page instead of signed in as `root`.
    signed_out: bool,
    /// Supply trusted Cloudflare credentials on a scratch server.
    cloudflare: bool,
    /// A regular user without a password, `alice`, with the unused login link
    /// [`LOGIN_LINK`].
    login_link: bool,
}

/// The token of the login link that `(login_link)` creates.
const LOGIN_LINK: &str = "e2e-login-link";

/// The user and login link for `(login_link)`, valid for a day.
const LOGIN_LINK_SQL: &str = "
    INSERT INTO users (login, email, role, created_at)
    VALUES ('alice', 'alice@example.com', 'regular', CAST(unixepoch('subsec') * 1000 AS INTEGER));
    INSERT INTO login_tokens (id, user_id, expires_at)
    VALUES ('e2e-login-link', last_insert_rowid(), CAST((unixepoch('subsec') + 86400) * 1000 AS INTEGER));
";

/// A track server for one test.
struct Track {
    port: u16,
    server: Option<Server>,
    // Dropped after the server has stopped, taking its database with it.
    dir: TempDir,
}

impl Fixture for Track {
    type Setup = Setup;

    fn config() -> Config {
        Config::default().about("Browser tests for track.")
    }

    /// The servers run in this process; only their warnings reach the output.
    fn init_tracing() {
        let filter = Targets::new()
            .with_default(LevelFilter::INFO)
            .with_target("track", LevelFilter::WARN);

        _ = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(filter)
            .try_init();
    }

    async fn start(setup: Setup) -> Result<Self> {
        let dir = TempDir::new()?;

        let seeded = setup.seeded
            || setup.crowded
            || setup.movie
            || setup.seasons
            || setup.upcoming
            || setup.untracked
            || setup.graphics
            || setup.backdrop
            || setup.next
            || setup.numbering
            || setup.remotes;

        if seeded || setup.login_link || setup.cloudflare {
            // The server creates the schema; the seed goes in while it is down.
            Server::start(dir.path(), yew_e2e::dist()?)
                .await?
                .quit()
                .await?;

            let c = sqll::Connection::open(dir.path().join("track.db"))?;

            if seeded {
                c.execute(SEED).context("seeding the database")?;
            }

            if setup.crowded {
                c.execute(CROWDED).context("crowding the database")?;
            }

            if setup.movie {
                c.execute(MOVIE).context("adding the movie")?;
            }

            if setup.seasons {
                c.execute(SEASONS).context("adding the seasons")?;
            }

            if setup.upcoming {
                c.execute(UPCOMING).context("adding the upcoming show")?;
            }

            if setup.untracked {
                c.execute(UNTRACKED).context("adding the untracked show")?;
            }

            if setup.graphics {
                c.execute(GRAPHICS).context("adding the graphics")?;
            }

            if setup.backdrop {
                c.execute(BACKDROP).context("adding the backdrop")?;
            }

            if setup.movie_backdrop {
                c.execute(MOVIE_BACKDROP)
                    .context("adding the movie backdrop")?;
            }

            if setup.next {
                c.execute(NEXT).context("adding the next-episode shows")?;
            }

            if setup.numbering {
                c.execute(NUMBERING).context("adding the XEM numbering")?;
            }

            if setup.remotes {
                c.execute(REMOTES).context("adding the remotes")?;
            }

            if setup.cloudflare {
                c.execute(
                    "UPDATE users SET email = 'root@example.com' WHERE login = 'root';
                    INSERT OR REPLACE INTO config (key, value) VALUES
                    ('cloudflare_access_enabled', 'true'),
                    ('cloudflare_team_domain', 'example.cloudflareaccess.com'),
                    ('cloudflare_audience', 'aud'),
                    ('cloudflare_trust_email_header', 'true'),
                    ('cloudflare_verify_jwt', 'false');",
                )?;
            }

            if setup.login_link {
                c.execute(LOGIN_LINK_SQL).context("adding the login link")?;
            }
        }

        let dist = if setup.cloudflare {
            cloudflare_dist()?
        } else if setup.signed_out {
            yew_e2e::dist()?
        } else {
            signed_in_dist()?
        };

        let server = Server::start(dir.path(), dist).await?;

        Ok(Self {
            port: server.port,
            server: Some(server),
            dir,
        })
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    async fn quit(mut self) -> Result<()> {
        self.stop().await
    }
}

impl Track {
    /// The first column of every row `sql` selects from the server's database.
    fn query(&self, sql: &str) -> Result<Vec<String>> {
        let c = sqll::Connection::open(self.dir.path().join("track.db"))?;
        let mut q = c.prepare(sql)?;
        let mut out = Vec::new();

        while let Some(value) = q.next::<String>()? {
            out.push(value);
        }

        Ok(out)
    }

    /// Stop the server before the test ends, dropping the browser's connection.
    async fn stop(&mut self) -> Result<()> {
        match self.server.take() {
            Some(server) => server.quit().await,
            None => Ok(()),
        }
    }
}

/// A server running in this process against the database in a directory.
struct Server {
    port: u16,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<Result<ExitCode>>,
}

/// The frontend with a script that signs in as `root` before the app starts,
/// unless the page already has a session: a copy of the build whose
/// `index.html` carries the script and whose other files link to the build's.
fn signed_in_dist() -> Result<&'static Path> {
    static DIST: OnceLock<PathBuf> = OnceLock::new();

    if let Some(dist) = DIST.get() {
        return Ok(dist);
    }

    let script = "<head><script>(() => {
        const me = new XMLHttpRequest();
        me.open('GET', '/api/auth/me', false);
        me.send();
        if (me.status !== 401) return;
        const login = new XMLHttpRequest();
        login.open('POST', '/api/auth/login', false);
        login.setRequestHeader('Content-Type', 'application/json');
        login.send(JSON.stringify({ login: 'root', password: 'root' }));
    })();</script>";

    let dist = scripted_dist("e2e-signed-in-dist", script)?;
    Ok(DIST.get_or_init(|| dist))
}

fn cloudflare_dist() -> Result<&'static Path> {
    static DIST: OnceLock<PathBuf> = OnceLock::new();
    if let Some(dist) = DIST.get() {
        return Ok(dist);
    }
    let script = "<head><script>(() => {
        const fetch = window.fetch.bind(window);
        window.fetch = (input, init) => {
            const request = new Request(input, init);
            request.headers.set('cf-access-authenticated-user-email', 'root@example.com');
            return fetch(request);
        };
    })();</script>";
    let dist = scripted_dist("e2e-cloudflare-dist", script)?;
    Ok(DIST.get_or_init(|| dist))
}

fn scripted_dist(name: &str, script: &str) -> Result<PathBuf> {
    let built = yew_e2e::dist()?;
    let dist = yew_e2e::target_dir()?.join(name);

    if dist.exists() {
        std::fs::remove_dir_all(&dist)?;
    }

    std::fs::create_dir_all(&dist)?;

    for entry in std::fs::read_dir(built)? {
        let entry = entry?;

        if entry.file_name() != "index.html" {
            std::os::unix::fs::symlink(entry.path(), dist.join(entry.file_name()))?;
        }
    }

    let index = std::fs::read_to_string(built.join("index.html"))?;
    ensure!(index.contains("<head>"), "index.html has no <head>");
    std::fs::write(dist.join("index.html"), index.replacen("<head>", script, 1))?;
    Ok(dist)
}

impl Server {
    async fn start(dir: &Path, dist: &'static Path) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let (shutdown, rx) = oneshot::channel::<()>();

        let db = dir.join("track.db");
        let cache = dir.join("image-cache");

        let task = tokio::spawn(async move {
            track::serve(listener, &db, &cache, Some(dist), async move {
                _ = rx.await;
            })
            .await
        });

        Ok(Self {
            port,
            shutdown,
            task,
        })
    }

    async fn quit(self) -> Result<()> {
        _ = self.shutdown.send(());
        let code = self.task.await.context("track panicked")??;
        ensure!(code == ExitCode::SUCCESS, "track stopped with {code:?}");
        Ok(())
    }
}

/// Tag the elements `selector` matches with their text, returning how many
/// there are, so [`kept_marked`] can tell later whether a render kept them or
/// replaced or refilled them.
async fn mark(driver: &TestDriver, selector: &str) -> Result<usize> {
    let ret = driver
        .webdriver()
        .execute(
            "const all = document.querySelectorAll(arguments[0]); \
             for (const e of all) e.__marked = e.textContent; \
             return all.length;",
            vec![selector.into()],
        )
        .await?;
    Ok(ret.convert()?)
}

/// How many elements [`mark`] tagged are still in the page with the same text.
async fn kept_marked(driver: &TestDriver, selector: &str) -> Result<usize> {
    let ret = driver
        .webdriver()
        .execute(
            "return [...document.querySelectorAll(arguments[0])].filter(e => e.__marked === e.textContent).length;",
            vec![selector.into()],
        )
        .await?;
    Ok(ret.convert()?)
}

yew_e2e::harness! {
    Track;
    auth::{cloudflare_requires_a_click(cloudflare), cloudflare_is_unavailable(signed_out), signs_in(signed_out), rejects_a_wrong_password(signed_out), signs_out(signed_out), registers_with_a_login_link(signed_out, login_link), changes_the_password(signed_out), changes_the_login},
    dashboard::{fills_rows_with_relative_dates(seeded), labels_its_filters, keeps_view_options_in_a_menu, buttons_expose_their_state, secondary_actions_are_filled(seeded), mobile_cards_always_have_a_picture(seeded), schedule_names_its_days, upcoming_is_an_agenda, upcoming_times_open_their_episode(upcoming), upcoming_keeps_days_when_more_are_shown(upcoming), schedule_keeps_weeks_when_more_are_shown(upcoming), schedule_entries_sit_flush_left(upcoming), marks_watched_in_one_click(seeded), marks_watched_at_a_chosen_time(seeded)},
    help::{opens_from_the_toolbar, search_filters_sections, links_between_sections, inline_help_opens_its_section, phones_stack_the_sections},
    media::{shows_a_poster_grid(seeded), partly_watched_shows_are_marked(seeded), toggle_marks_are_icon_sized, sort_stays_readable_at_tablet_width, reversing_keeps_the_cards(crowded), lists_tracked_items_by_default(untracked), filters_by_next_episode(next, movie), marks_the_next_episode_of_picked_shows(next), picked_items_track_and_untrack(next, movie), phone_pick_control_is_round(seeded)},
    movie::{puts_the_cast_beside_the_poster(movie), phone_release_line_stays_together(movie), phones_show_the_backdrop_once(movie, movie_backdrop)},
    navigation::{phone_menu_rows_align, opens_every_page, tab_shows_a_focus_ring, navigation_is_links(seeded), pages_have_landmarks_and_one_heading, page_scrolls_the_window, toolbar_icons_are_small, app_bar_items_have_room, errors_show_as_a_card, every_button_has_a_title(seeded)},
    people::{lists_people_by_credits(seeded), shows_no_count_while_loading(seeded), shows_a_silhouette_without_a_photo(seeded), phone_person_page_keeps_the_photo_shape(seeded), known_for_lists_each_title_once(seeded), edits_a_persons_remotes(seeded)},
    queue::{lists_tasks_in_columns, empty_filters_say_what_is_missing, keeps_rows_in_place(seeded), follows_the_next_task(crowded), shows_failed_tasks(seeded)},
    search::{focuses_the_input, says_what_it_searches},
    settings::{reorders_sync_sources, reorders_xem_lookup, shows_and_hides_numberings, theme_applies_live, theme_is_remembered, adds_languages_and_rules, switches_work_from_the_keyboard, fields_follow_the_theme, settings_are_labelled_rows, tab_completes_the_time_zone, configures_cloudflare_access, warns_about_trusting_only_the_email_header, phones_list_the_settings_pages},
    users::{creates_a_user_who_registers, regular_users_do_not_see_users(login_link), changes_a_role(login_link), revokes_links_and_deletes(login_link), regular_users_cannot_remove_media(seeded, movie, login_link), regular_users_cannot_edit_shared_data(seeded, movie, login_link)},
    sync::{spinner_survives_a_reload(crowded, remotes), spinner_opens_the_task_in_the_queue(crowded, remotes)},
    show::{rail_colours_watched_apart_from_pending(seeded), phone_action_rows_align(seeded), episode_menu_holds_the_other_actions(seeded), menus_close_on_an_outside_click(seeded), modals_close_from_button_and_backdrop(seeded), phone_modals_rise_from_the_bottom(seeded), settings_line_up_their_controls(seeded), modals_hold_keyboard_focus(seeded), menus_work_from_the_keyboard(seeded), mark_watched_is_one_colour(seeded), seasons_list_beside_the_episodes(seasons), phones_pick_seasons_from_chips(seasons), season_overview_switches_language(seasons), phone_popovers_are_sheets(seeded), watch_history_moves_and_removes(seeded), tracking_toggle_names_the_show(seeded), settings_pages_return_to_settings(seeded), translations_sit_beside_their_language(seeded), air_dates_explain_the_default_quietly(seeded), graphics_say_what_they_do(graphics), detail_poster_is_rounded(graphics), remotes_show_their_actions(seeded), xem_anidb_and_scene_remotes_are_added(seeded), tvmaze_remote_is_added(seeded), picked_episodes_are_marked_together(seeded), picked_episodes_clear(seasons), remaining_episodes_advance_pending(seasons), has_a_heading(seeded), phones_have_no_episode_rail(seeded), phones_do_not_scroll_sideways(seeded), phones_show_the_backdrop_once(backdrop), seasons_count_watched_episodes(seeded), episodes_show_their_details(seeded), numbering_ranges_are_edited(numbering), alternative_names_are_listed(numbering), episodes_show_other_numberings(numbering), xem_remote_links_to_its_page(numbering)},
}
