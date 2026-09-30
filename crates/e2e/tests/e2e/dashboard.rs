use yew_e2e::prelude::*;

use super::Track;

const FIRST: &str = "S01E01 First Episode";
const SECOND: &str = "S01E02 Second Episode";

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
        .wait_texts(".tabs .tab", ["What's Next", "Upcoming", "Schedule"])
        .await?;
    driver
        .wait_texts(".chips .chip.selected", ["Shows", "Movies"])
        .await?;

    driver
        .find_one_by("[title='Show movies']")
        .await?
        .click()
        .await?;
    driver.wait_texts(".chips .chip.selected", ["Shows"]).await
}

/// The lookahead and page size wait behind a View options button instead of
/// crowding the controls above the cards.
pub async fn keeps_view_options_in_a_menu(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.find_one_by("[title='View options']").await?;
    ensure!(
        driver.count("input[type=number]").await? == 0,
        "the lookahead shows before View options is opened"
    );

    driver
        .find_one_by("[title='View options']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".context-menu label", ["LOOK AHEAD", "PER PAGE"])
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

/// The schedule names every day's weekday in its own cell, with no separate
/// weekday header, under a heading naming the months shown.
pub async fn schedule_names_its_days(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(1200, 900).await?;
    driver.reopen_with("view=schedule").await?;

    let heading = driver.find_first(".calendar-month").await?.text().await?;
    ensure!(
        heading.chars().any(|c| c.is_ascii_digit()),
        "the month heading has no year: {heading:?}"
    );

    driver.wait_count(".calendar-weekdays", 0).await?;

    let days = driver.find_all_texts(".calendar-cell .day-of-week").await?;
    ensure!(
        days.len() >= 7,
        "expected every day to be labelled, got {days:?}"
    );

    ensure!(
        days.iter().all(|day| !day.trim().is_empty()),
        "a day has no weekday: {days:?}"
    );

    Ok(())
}

/// On a phone every What's Next card has a picture, even a show without a
/// banner (the seeded show has no images at all).
pub async fn mobile_cards_always_have_a_picture(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    driver.wait_texts(".pending-label", [FIRST]).await?;

    let banner = driver.find_one_by(".pending-item image.banner").await?;
    ensure!(banner.visible().await?, "the card has no picture");
    ensure!(
        banner.rect().await?.height > 0.0,
        "the card's picture is empty"
    );
    Ok(())
}
