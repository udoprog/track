use yew_e2e::prelude::*;

use super::Track;

/// Every page of the history in `history.sql` but the last is full.
const PAGE: usize = 30;

/// Open the dashboard's History tab and wait for its first page.
async fn open(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by(".track-tab[title='History']")
        .await?
        .click()
        .await?;
    driver.wait_count(".track-history-item", PAGE).await
}

/// History lists every watch newest first, rewatches included, and folds the
/// episodes watched back to back into one entry that unfolds into them.
pub async fn folds_episodes_watched_back_to_back(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    driver.set_window_size(1200, 900).await?;
    open(driver).await?;

    let titles = driver
        .rendered_texts(".track-history-item .track-schedule-title")
        .await?;
    ensure!(
        titles.starts_with(&["Seeded Show", "Seeded Movie", "Seeded Show"].map(String::from)),
        "the newest watches are not first: {titles:?}"
    );

    // The rewatch stands alone; the run before it is folded.
    driver
        .wait_texts(
            ".track-history-item:not(:has(.track-history-toggle)) .track-badge",
            {
                let mut badges = ["Movie"; PAGE - 1];
                badges[0] = "S01E01";
                badges
            },
        )
        .await?;

    let toggle = driver.find_one_by(".track-history-toggle").await?;
    ensure!(
        toggle.text().await? == "3 episodes",
        "the run does not say how many episodes it holds"
    );
    ensure!(toggle.attr("aria-expanded").await? == "false");
    driver
        .wait_texts(
            ".track-history-item:has(.track-history-toggle) .track-schedule-times .track-badge",
            ["S01E01–S01E03"],
        )
        .await?;

    toggle.click().await?;
    driver
        .wait_texts(
            ".track-history-episodes .track-badge",
            ["S01E03", "S01E02", "S01E01"],
        )
        .await?;
    driver
        .wait_texts(
            ".track-history-episodes .history-episode-title",
            [
                "Unbreakablyconcatenatedepisodename",
                "Second Episode",
                "First Episode",
            ],
        )
        .await?;
    ensure!(
        driver
            .find_one_by(".track-history-toggle")
            .await?
            .attr("aria-expanded")
            .await?
            == "true"
    );

    let href = driver
        .find_one_by(".track-history-episodes li:nth-child(2) a")
        .await?
        .attr("href")
        .await?;
    ensure!(
        href.starts_with("/shows/") && href.ends_with("#S01E02"),
        "the episode does not link to itself: {href}"
    );

    driver
        .find_one_by(".track-history-toggle")
        .await?
        .click()
        .await?;
    driver
        .wait_count_from(".track-history-episodes li", 3, 0)
        .await
}

/// History pages by entry, the media-kind toggle and the title filter narrow
/// it from the first page, and the filter is kept in the address.
pub async fn pages_and_filters_the_history(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open(driver).await?;
    ensure!(
        driver.count("#page .sockel-pager [title='Page 3']").await? == 0,
        "a page past the next one is offered"
    );

    driver
        .find_first("#page .sockel-pager [title='Page 2']")
        .await?
        .click()
        .await?;
    // 3 entries of the show and the newest movie, then 35 older movies.
    driver
        .wait_count_from(".track-history-item", PAGE, 8)
        .await?;

    let before = driver.count(".track-history-item").await?;
    driver
        .find_one_by("[title='Showing movies']")
        .await?
        .click()
        .await?;
    // Without the movie between them the rewatch still stands alone: it is a
    // day after the run.
    driver
        .wait_count_from(".track-history-item", before, 2)
        .await?;
    driver
        .wait_texts(
            ".track-history-item .track-schedule-title",
            ["Seeded Show", "Seeded Show"],
        )
        .await?;

    driver
        .find_one_by("[title='Hiding movies']")
        .await?
        .click()
        .await?;
    driver.wait_count(".track-history-item", PAGE).await?;

    driver
        .find_one_by("input[aria-label='Filter history']")
        .await?
        .send_keys("SHOW")
        .await?;
    driver
        .wait_count_from(".track-history-item", PAGE, 2)
        .await?;

    driver.reload().await?;
    driver.wait_count(".track-history-item", 2).await?;
    let filter = driver
        .webdriver()
        .execute(
            "return document.querySelector(\"input[aria-label='Filter history']\").value;",
            Vec::new(),
        )
        .await?
        .convert::<String>()?;
    ensure!(filter == "SHOW", "the filter is not kept: {filter:?}");

    driver
        .find_one_by("input[aria-label='Filter history']")
        .await?
        .send_keys(" nothing")
        .await?;
    driver
        .wait_texts("#page .track-item-inline-more", ["Nothing to show."])
        .await
}

/// On a phone an unfolded run fits the width: long episode titles are cut
/// short instead of making the page scroll sideways.
pub async fn phone_history_fits_the_width(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    driver.reopen_with("view=history").await?;
    driver.wait_count(".track-history-item", PAGE).await?;

    driver
        .find_one_by(".track-history-toggle")
        .await?
        .click()
        .await?;
    driver.wait_count(".track-history-episodes li", 3).await?;

    let ret = driver
        .webdriver()
        .execute(
            "const e = document.scrollingElement; return e.scrollWidth <= e.clientWidth;",
            Vec::new(),
        )
        .await?;
    ensure!(ret.convert::<bool>()?, "the page scrolls sideways");
    Ok(())
}
