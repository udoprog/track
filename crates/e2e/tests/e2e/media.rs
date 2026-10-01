use yew_e2e::prelude::*;

use super::{Track, kept_marked, mark};

/// The Media page is a grid of posters saying what each item is and whether
/// it has been watched.
pub async fn shows_a_poster_grid(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;

    driver.wait_texts(".media-title", ["Seeded Show"]).await?;
    driver
        .find_one_by(".media-card .media-poster image")
        .await?;

    let meta = driver.find_all_texts(".media-card .media-meta").await?;

    ensure!(
        meta.iter().any(|m| m == "2023") && meta.iter().any(|m| m == "Not watched"),
        "expected the year and watch state, got {meta:?}"
    );

    Ok(())
}

/// The check marks on toggles are icon sized, not as tall as the toggle, on
/// a phone too.
pub async fn toggle_marks_are_icon_sized(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    driver.wait_texts(".site-title", ["Track"]).await?;

    driver.find_one_by(".toolbar-toggle").await?.click().await?;
    driver
        .find_one_by(".toolbar-item[title=Settings]")
        .await?
        .click()
        .await?;

    driver.find_first(".input-checkbox .mark").await?;
    let marks = driver.find_all(By::Css(".input-checkbox .mark")).await?;

    for mark in marks {
        let rect = mark.rect().await?;

        ensure!(
            rect.height <= 20.0,
            "a toggle mark is {}px tall, expected at most 20px",
            rect.height
        );
    }

    Ok(())
}

/// Between phone and full width the controls wrap rather than squeezing the
/// sort chip, and the filter field stays usable.
pub async fn sort_stays_readable_at_tablet_width(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    driver.set_window_size(860, 900).await?;

    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;

    let chip = driver.find_one_by("[title='Sort by']").await?;
    ensure!(
        chip.text().await? == "Title",
        "the sort chip reads {:?}",
        chip.text().await?
    );
    let sort = chip.rect().await?;
    let filter = driver.find_one_by(".search-field").await?.rect().await?;

    ensure!(
        sort.height <= 40.0,
        "the sort chip wraps to {}px",
        sort.height
    );
    ensure!(
        filter.width >= 200.0,
        "the filter field is {}px wide",
        filter.width
    );
    Ok(())
}

/// A show with aired episodes still to watch carries a yellow watched mark;
/// once every regular episode is watched the mark is the plain one.
pub async fn partly_watched_shows_are_marked(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".pending-title", ["Seeded Show"]).await?;

    // Watch the first episode from the dashboard, then look at Media.
    driver
        .find_one_by(".pending-item [title='Mark watched']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".pending-label", ["S01E02 Second Episode"])
        .await?;

    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;

    let badge = driver.find_one_by(".media-card .media-badge").await?;
    ensure!(badge.attr("class").await?.contains("partial"));
    ensure!(badge.attr("title").await? == "Partly watched: 2 episodes to go");
    Ok(())
}

/// Reversing the order moves the cards instead of refilling each one with
/// another title.
pub async fn reversing_keeps_the_cards(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    const CARDS: &str = ".media-card";

    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;
    driver.find_first(CARDS).await?;

    let titles = driver.rendered_texts(".media-title").await?;
    let marked = mark(driver, CARDS).await?;

    driver
        .find_one_by("[title='Ascending']")
        .await?
        .click()
        .await?;

    driver
        .wait_until("the order to reverse", async || {
            let mut now = driver.rendered_texts(".media-title").await?;
            now.reverse();
            Ok(now == titles)
        })
        .await?;

    let kept = kept_marked(driver, CARDS).await?;
    ensure!(
        kept == marked,
        "only {kept} of the {marked} cards were kept"
    );
    Ok(())
}

/// A fresh visit lists only what the user tracks; All shows the rest.
pub async fn lists_tracked_items_by_default(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;
    driver.wait_texts(".media-title", ["Seeded Show"]).await?;

    driver
        .find_one_by("[title='Showing: Tracked']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".media-title", ["Untracked Show"])
        .await?;

    driver
        .find_one_by("[title='Showing: Untracked']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".media-title", ["Seeded Show", "Untracked Show"])
        .await?;

    let url = driver.webdriver().current_url().await?;
    ensure!(url.as_str().contains("tracked=all"), "the URL is {url}");
    Ok(())
}

/// The next-episode filter keeps the shows with an episode up next in the
/// regular seasons or the specials, hides movies, and survives a reload.
pub async fn filters_by_next_episode(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(
            ".media-title",
            [
                "Finished Show",
                "Rewatch Show",
                "Seeded Movie",
                "Seeded Show",
                "Specials Show",
            ],
        )
        .await?;

    driver
        .find_one_by("[title='Next episode: any']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".media-title", ["Rewatch Show", "Seeded Show"])
        .await?;

    driver
        .find_one_by("[title='Next episode: regular']")
        .await?
        .click()
        .await?;
    driver.wait_texts(".media-title", ["Specials Show"]).await?;

    let url = driver.webdriver().current_url().await?;
    ensure!(url.as_str().contains("next=specials"), "the URL is {url}");

    driver.reload().await?;
    driver.wait_texts(".media-title", ["Specials Show"]).await?;
    driver
        .find_one_by("[title='Next episode: specials']")
        .await?;
    Ok(())
}

const ESCAPE: &str = "\u{E00C}";

/// The episodes of a show that root has watched, oldest first.
fn watched(track: &Track, show: u64) -> Result<Vec<String>> {
    track.query(&format!(
        "SELECT printf('S%02dE%02d', season, episode) FROM watched_episodes \
         WHERE show_id = {show} ORDER BY timestamp, id"
    ))
}

/// The titles of the shows root tracks, in order.
fn tracked_shows(track: &Track) -> Result<Vec<String>> {
    track.query(
        "SELECT text FROM show_strings JOIN user_tracked_shows USING (show_id) \
         WHERE kind = 1 ORDER BY text",
    )
}

async fn mark_next(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by(".selection-bar [title='Mark the next episode of the selected shows watched']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".context-menu [title=Confirm]")
        .await?
        .click()
        .await
}

/// Mark next marks each picked show's next episode in the scope the bar
/// picks, continues a rewatch where it is, and says how many shows had no
/// next episode.
pub async fn marks_the_next_episode_of_picked_shows(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(
            ".media-title",
            [
                "Finished Show",
                "Rewatch Show",
                "Seeded Show",
                "Specials Show",
            ],
        )
        .await?;

    driver
        .find_one_by("[title='Select Finished Show']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='Select Specials Show']")
        .await?
        .shift_click()
        .await?;

    driver
        .wait_texts(".selection-count", ["4 shows selected"])
        .await?;
    ensure!(driver.count(".media-pick[aria-pressed=true]").await? == 4);
    driver
        .find_one_by(".selection-bar [title='From the regular seasons'][aria-pressed=true]")
        .await?;

    mark_next(driver).await?;
    driver.wait_count(".selection-bar", 0).await?;
    driver
        .wait_texts(
            ".toast .fill",
            ["Marked the next episode of 2 shows; 2 shows had no next episode"],
        )
        .await?;

    ensure!(watched(track, 1001)? == ["S01E01"]);
    ensure!(watched(track, 1003)? == ["S01E01"]);
    ensure!(watched(track, 1004)? == ["S01E01"]);
    ensure!(
        watched(track, 1005)? == ["S01E01", "S01E02", "S01E01", "S01E02"],
        "the rewatch went on to {:?}",
        watched(track, 1005)?
    );

    driver
        .find_one_by("[title='Select Specials Show']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='Select Seeded Show']")
        .await?
        .click()
        .await?;
    driver.wait_count(".toast", 0).await?;
    driver
        .find_one_by(".selection-bar [title='From the specials']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".selection-bar [title='From the specials'][aria-pressed=true]")
        .await?;

    mark_next(driver).await?;
    driver
        .wait_texts(
            ".toast .fill",
            ["Marked the next episode of 1 show; 1 show had no next episode"],
        )
        .await?;

    ensure!(watched(track, 1004)? == ["S01E01", "S00E01"]);
    ensure!(watched(track, 1001)? == ["S01E01"]);
    Ok(())
}

/// The selection bar offers only what applies to the picked items: tracking
/// what is untracked, untracking what is tracked, and marking movies or the
/// next episode of shows. Escape clears it, and Mark next takes its scope
/// from the next-episode filter.
pub async fn picked_items_track_and_untrack(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    const MARK_NEXT: &str =
        ".selection-bar [title='Mark the next episode of the selected shows watched']";
    const MARK_MOVIES: &str = ".selection-bar [title='Mark the selected movies watched']";
    const TRACK: &str = ".selection-bar [title='Track the selected items']";
    const UNTRACK: &str = ".selection-bar [title='Stop tracking the selected items']";

    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            ".media-title",
            [
                "Finished Show",
                "Rewatch Show",
                "Seeded Movie",
                "Seeded Show",
                "Specials Show",
            ],
        )
        .await?;

    driver
        .find_one_by("[title='Select Seeded Movie']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".selection-count", ["1 movie selected"])
        .await?;
    ensure!(driver.count(MARK_NEXT).await? == 0);
    ensure!(driver.count(MARK_MOVIES).await? == 1);
    ensure!(driver.count(TRACK).await? == 0);
    ensure!(driver.count(UNTRACK).await? == 1);

    driver
        .find_one_by("[title='Select Seeded Show']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".selection-count", ["1 show and 1 movie selected"])
        .await?;
    ensure!(driver.count(MARK_NEXT).await? == 1);

    driver
        .find_one_by("[title='Select Seeded Movie']")
        .await?
        .send_keys(ESCAPE)
        .await?;
    driver.wait_count(".selection-bar", 0).await?;
    ensure!(driver.count(".media-pick[aria-pressed=true]").await? == 0);

    driver
        .find_one_by("[title='Select Finished Show']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='Select Rewatch Show']")
        .await?
        .click()
        .await?;
    driver.find_one_by(UNTRACK).await?.click().await?;
    driver
        .wait_texts(
            ".media-title",
            ["Seeded Movie", "Seeded Show", "Specials Show"],
        )
        .await?;
    ensure!(tracked_shows(track)? == ["Seeded Show", "Specials Show"]);

    for title in ["Showing: Tracked", "Showing: Untracked"] {
        driver
            .find_one_by(&format!("[title='{title}']"))
            .await?
            .click()
            .await?;
    }

    driver
        .find_one_by("[title='Select Finished Show']")
        .await?
        .click()
        .await?;
    ensure!(driver.count(TRACK).await? == 1);
    ensure!(driver.count(UNTRACK).await? == 0);

    driver
        .find_one_by("[title='Select Seeded Show']")
        .await?
        .click()
        .await?;
    ensure!(driver.count(UNTRACK).await? == 1);

    driver.find_one_by(TRACK).await?.click().await?;
    driver.wait_count(".selection-bar", 0).await?;
    driver
        .wait_until("Finished Show to be tracked", async || {
            Ok(tracked_shows(track)? == ["Finished Show", "Seeded Show", "Specials Show"])
        })
        .await?;

    for title in ["Next episode: any", "Next episode: regular"] {
        driver
            .find_one_by(&format!("[title='{title}']"))
            .await?
            .click()
            .await?;
    }

    driver.wait_texts(".media-title", ["Specials Show"]).await?;
    driver
        .find_one_by("[title='Select Specials Show']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".selection-bar [title='From the specials'][aria-pressed=true]")
        .await?;
    Ok(())
}

/// A card's select control stays a circle on a phone.
pub async fn phone_pick_control_is_round(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    driver.wait_texts(".site-title", ["Track"]).await?;

    driver.find_one_by(".toolbar-toggle").await?.click().await?;
    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;

    let rect = driver
        .find_one_by("[title='Select Seeded Show']")
        .await?
        .rect()
        .await?;

    ensure!(
        rect.width == rect.height,
        "the select control is {}x{}px",
        rect.width,
        rect.height
    );
    Ok(())
}
