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
