use yew_e2e::prelude::*;

use super::Track;

const FIRST: &str = "S01E01 ─ First Episode";
const SECOND: &str = "S01E02 ─ Second Episode";

/// The watched button marks the episode in one click and moves the show along,
/// and the toast undoes it.
pub async fn marks_watched_in_one_click(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".pending-title", ["Seeded Show"]).await?;
    driver.wait_texts(".pending-label", [FIRST]).await?;

    driver
        .find_one_by(".pending-item [title='Mark watched']")
        .await?
        .click()
        .await?;

    driver.wait_texts(".pending-label", [SECOND]).await?;
    driver
        .find_one_by(".toast [title=Undo]")
        .await?
        .click()
        .await?;
    driver.wait_texts(".pending-label", [FIRST]).await?;
    driver.wait_count(".toast", 0).await?;
    Ok(())
}

/// The picker beside the watched button opens below it, stays there when the
/// custom picker expands, and marks the episode on confirm.
pub async fn marks_watched_at_a_chosen_time(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".pending-label", [FIRST]).await?;

    let more = driver
        .find_one_by(".pending-item [title='Choose when']")
        .await?;
    more.click().await?;

    let menu = driver.find_one_by(".context-menu").await?;

    driver
        .find_one_by(By::XPath(
            "//div[contains(@class, 'context-menu')]//button[normalize-space()='Custom']",
        ))
        .await?
        .click()
        .await?;

    driver.find_one_by(".mark-time-dial").await?;

    let trigger = more.rect().await?;
    let popover = menu.rect().await?;

    ensure!(
        popover.y >= trigger.y + trigger.height,
        "the picker covers its trigger: popover at {}, trigger ends at {}",
        popover.y,
        trigger.y + trigger.height
    );

    driver
        .find_one_by(".context-menu [title=Confirm]")
        .await?
        .click()
        .await?;

    driver.wait_texts(".pending-label", [SECOND]).await?;
    Ok(())
}

/// The dashboard's filters say what they filter on wide screens too.
pub async fn labels_its_filters(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .wait_texts(".input-checkbox", ["Shows", "Movies"])
        .await
}

/// What's Next fits as many cards in a row as the width allows and says how
/// long ago each became available, with the exact date on hover.
pub async fn fills_rows_with_relative_dates(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(1200, 900).await?;
    driver.wait_texts(".pending-label", [FIRST]).await?;

    let grid = driver.find_one_by(".pending-grid").await?;

    driver
        .wait_until("several cards to fit in a row", async || {
            let style = grid.attr("style").await?;

            let columns = style
                .trim_start_matches("--pending-columns:")
                .trim()
                .parse::<usize>()
                .unwrap_or(0);

            Ok(columns >= 4)
        })
        .await?;

    let date = driver.find_one_by(".pending-date").await?;
    let text = date.text().await?;
    ensure!(
        text.ends_with(" ago"),
        "expected a relative date, got {text:?}"
    );
    ensure!(
        !date.attr("title").await?.is_empty(),
        "the date has no exact form on hover"
    );
    Ok(())
}

/// Upcoming gives every day a readable width, scrolling sideways instead of
/// squeezing the days when they do not fit.
pub async fn upcoming_days_keep_their_width(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(800, 900).await?;
    driver.reopen_with("view=upcoming").await?;

    driver
        .wait_count(".schedule-range-grid .schedule-range-poster", 0)
        .await?;

    let day = driver
        .find_first(".schedule-range-grid > .calendar-cell")
        .await?;

    let width = day.rect().await?.width;
    ensure!(width >= 160.0, "a day is only {width}px wide");
    Ok(())
}
