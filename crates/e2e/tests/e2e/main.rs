//! Browser tests for track. Every test gets a server of its own, with a fresh
//! database and image cache in the run's sandbox, serving the frontend that
//! yew-e2e builds for the run. A fresh database has no API keys, so nothing is
//! synced from the remotes.
//!
//! ```text
//! cargo test -p e2e
//! cargo test -p e2e -- settings::
//! cargo test -p e2e -- --headed --last-session
//! ```

mod dashboard;
mod media;
mod movie;
mod navigation;
mod people;
mod queue;
mod search;
mod settings;
mod show;

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
use yew_e2e::{Config, Fixture};

static SANDBOX: OnceLock<PathBuf> = OnceLock::new();

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
/// Poster candidates for the seeded show, with `(graphics)`.
const GRAPHICS: &str = include_str!("graphics.sql");

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
    /// Start with `seed.sql` and the poster candidates in `graphics.sql`.
    graphics: bool,
}

/// A track server for one test.
struct Track {
    port: u16,
    server: Option<Server>,
    // Dropped after the server has stopped, taking its database with it.
    _dir: TempDir,
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

    fn enter_sandbox(root: &Path) -> Result<()> {
        _ = SANDBOX.set(root.to_owned());
        Ok(())
    }

    async fn start(setup: Setup) -> Result<Self> {
        let sandbox = SANDBOX.get().context("the sandbox was not entered")?;
        let dir = TempDir::new_in(sandbox)?;

        if setup.seeded
            || setup.crowded
            || setup.movie
            || setup.seasons
            || setup.upcoming
            || setup.graphics
        {
            // The server creates the schema; the seed goes in while it is down.
            Server::start(dir.path()).await?.quit().await?;

            let c = sqll::Connection::open(dir.path().join("track.db"))?;
            c.execute(SEED).context("seeding the database")?;

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

            if setup.graphics {
                c.execute(GRAPHICS).context("adding the graphics")?;
            }
        }

        let server = Server::start(dir.path()).await?;

        Ok(Self {
            port: server.port,
            server: Some(server),
            _dir: dir,
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

impl Server {
    async fn start(dir: &Path) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let (shutdown, rx) = oneshot::channel::<()>();

        let db = dir.join("track.db");
        let cache = dir.join("image-cache");
        let dist = yew_e2e::dist()?;

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

yew_e2e::harness! {
    Track;
    dashboard::{fills_rows_with_relative_dates(seeded), labels_its_filters, keeps_view_options_in_a_menu, buttons_expose_their_state, secondary_actions_are_filled(seeded), mobile_cards_always_have_a_picture(seeded), schedule_names_its_days, upcoming_is_an_agenda, upcoming_times_open_their_episode(upcoming), schedule_entries_sit_flush_left(upcoming), marks_watched_in_one_click(seeded), marks_watched_at_a_chosen_time(seeded)},
    media::{shows_a_poster_grid(seeded), partly_watched_shows_are_marked(seeded), toggle_marks_are_icon_sized, sort_stays_readable_at_tablet_width},
    movie::{puts_the_cast_beside_the_poster(movie), phone_release_line_stays_together(movie)},
    navigation::{opens_every_page, tab_shows_a_focus_ring, navigation_is_links(seeded), pages_have_landmarks_and_one_heading, page_scrolls_the_window, toolbar_icons_are_small, app_bar_items_have_room, errors_show_as_a_card, every_button_has_a_title(seeded)},
    people::{lists_people_by_credits(seeded), shows_no_count_while_loading(seeded), shows_a_silhouette_without_a_photo(seeded), phone_person_page_keeps_the_photo_shape(seeded), known_for_lists_each_title_once(seeded)},
    queue::{lists_tasks_in_columns, empty_filters_say_what_is_missing, keeps_rows_in_place(seeded), follows_the_next_task(crowded), shows_failed_tasks(seeded)},
    search::{focuses_the_input, says_what_it_searches},
    settings::{reorders_sync_sources, theme_applies_live, theme_is_remembered, adds_languages_and_rules, switches_work_from_the_keyboard, fields_follow_the_theme, settings_are_labelled_rows, tab_completes_the_time_zone},
    show::{episode_menu_holds_the_other_actions(seeded), menus_close_on_an_outside_click(seeded), modals_close_from_button_and_backdrop(seeded), phone_modals_rise_from_the_bottom(seeded), settings_line_up_their_controls(seeded), modals_hold_keyboard_focus(seeded), menus_work_from_the_keyboard(seeded), mark_watched_is_one_colour(seeded), seasons_list_beside_the_episodes(seasons), phones_pick_seasons_from_chips(seasons), season_overview_switches_language(seasons), phone_popovers_are_sheets(seeded), watch_history_moves_and_removes(seeded), tracking_toggle_names_the_show(seeded), settings_pages_return_to_settings(seeded), translations_sit_beside_their_language(seeded), air_dates_explain_the_default_quietly(seeded), graphics_say_what_they_do(graphics), detail_poster_is_rounded(graphics), remotes_show_their_actions(seeded), picked_episodes_are_marked_together(seeded), picked_episodes_clear(seasons), has_a_heading(seeded), phones_have_no_episode_rail(seeded), phones_do_not_scroll_sideways(seeded), seasons_count_watched_episodes(seeded), episodes_show_their_details(seeded)},
}
