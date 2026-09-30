use yew_e2e::prelude::*;

use super::Track;

const FIRST: &str = "S01E01 First Episode";
const SECOND: &str = "S01E02 Second Episode";

/// Wait for the one card to read `label`. The card is replaced when it moves
/// to another episode, so read it in one snapshot rather than by handle.
async fn wait_label(driver: &TestDriver, label: &str) -> Result<()> {
    driver
        .wait_until(format_args!("the card to read {label:?}"), async || {
            Ok(driver.rendered_texts(".pending-label").await? == [label])
        })
        .await
}

/// The watched button marks the episode in one click and moves the show along,
/// and the toast undoes it.
pub async fn marks_watched_in_one_click(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".pending-title", ["Seeded Show"]).await?;
    wait_label(driver, FIRST).await?;

    driver
        .find_one_by(".pending-item [title='Mark watched']")
        .await?
        .click()
        .await?;

    wait_label(driver, SECOND).await?;
    driver
        .find_one_by(".toast [title=Undo]")
        .await?
        .click()
        .await?;
    wait_label(driver, FIRST).await?;
    driver.wait_count(".toast", 0).await?;
    Ok(())
}

/// The picker beside the watched button opens below it, stays there when the
/// custom picker expands, and marks the episode on confirm.
pub async fn marks_watched_at_a_chosen_time(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    wait_label(driver, FIRST).await?;

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

    wait_label(driver, SECOND).await?;
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
    wait_label(driver, FIRST).await?;

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
/// Upcoming is an agenda: a heading per day, starting today, and nothing to
/// scroll sideways.
pub async fn upcoming_is_an_agenda(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(800, 900).await?;
    driver.reopen_with("view=upcoming").await?;

    driver
        .wait_until("the first day to be today", async || {
            let days = driver.rendered_texts(".agenda-day-title").await?;
            Ok(days.first().is_some_and(|d| d.starts_with("Today")))
        })
        .await?;

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

/// The schedule names the weekdays once above the grid, under a heading
/// naming the months shown, and marks today.
pub async fn schedule_names_its_days(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(1200, 900).await?;
    driver.reopen_with("view=schedule").await?;

    let heading = driver.find_first(".calendar-month").await?.text().await?;
    ensure!(
        heading.chars().any(|c| c.is_ascii_digit()),
        "the month heading has no year: {heading:?}"
    );

    let weekdays = driver.rendered_texts(".calendar-weekdays > span").await?;
    ensure!(
        weekdays.len() == 7 && weekdays.iter().all(|d| !d.trim().is_empty()),
        "expected seven weekday names, got {weekdays:?}"
    );

    driver
        .find_one_by(".calendar-cell.today .calendar-day-number")
        .await?;
    Ok(())
}

/// On a phone every What's Next card has a picture, even a show without a
/// banner (the seeded show has no images at all).
pub async fn mobile_cards_always_have_a_picture(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    wait_label(driver, FIRST).await?;

    let banner = driver.find_one_by(".pending-item image.banner").await?;
    ensure!(banner.visible().await?, "the card has no picture");
    ensure!(
        banner.rect().await?.height > 0.0,
        "the card's picture is empty"
    );
    Ok(())
}
