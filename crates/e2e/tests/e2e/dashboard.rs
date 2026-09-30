use yew_e2e::prelude::*;

use super::{Track, kept_marked, mark};

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

    let date = driver.find_one_by(".context-menu input[type=date]").await?;
    driver.find_one_by(".context-menu input[type=time]").await?;

    let trigger = more.rect().await?;
    let popover = menu.rect().await?;

    ensure!(
        popover.y >= trigger.y + trigger.height,
        "the picker covers its trigger: popover at {}, trigger ends at {}",
        popover.y,
        trigger.y + trigger.height
    );

    // Pick a day and time as the browser's own fields would report them.
    driver
        .webdriver()
        .execute(
            "for (const [sel, value] of [['input[type=date]', '2024-01-02'], ['input[type=time]', '10:15']]) {
                 const input = document.querySelector('.context-menu ' + sel);
                 input.value = value;
                 input.dispatchEvent(new Event('change', { bubbles: true }));
             }",
            Vec::new(),
        )
        .await?;
    ensure!(date.value().await? == "2024-01-02");

    driver
        .find_one_by(".context-menu [title=Confirm]")
        .await?
        .click()
        .await?;

    wait_label(driver, SECOND).await?;

    // The episode was marked watched at the chosen time.
    driver.find_one_by(".pending-title").await?.click().await?;
    driver
        .wait_until("the first episode to show the chosen time", async || {
            let meta = driver.rendered_texts("[id='S01E01'] .episode-meta").await?;
            Ok(meta.iter().any(|m| {
                let words = m.split_whitespace().collect::<Vec<_>>().join(" ");
                words.contains("Watched once 2nd of January, 2024 at 10:15")
            }))
        })
        .await
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
        .wait_texts(".context-menu label", ["Look ahead", "Per page"])
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

/// Buttons say what state they are in: a popover trigger whether it is open,
/// a filter chip whether it is on.
pub async fn buttons_expose_their_state(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    let options = driver.find_one_by("[title='View options']").await?;

    ensure!(options.attr("aria-haspopup").await? == "dialog");
    ensure!(options.attr("aria-expanded").await? == "false");

    options.click().await?;
    driver.wait_count(".context-menu", 1).await?;

    driver
        .wait_until("View options to report itself open", async || {
            Ok(driver
                .find_one_by("[title='View options']")
                .await?
                .attr("aria-expanded")
                .await?
                == "true")
        })
        .await?;

    let shows = driver.find_one_by("[title='Show series']").await?;
    ensure!(shows.attr("aria-pressed").await? == "true");
    Ok(())
}

/// A card's secondary actions look like buttons: they have a fill before they
/// are hovered.
pub async fn secondary_actions_are_filled(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    wait_label(driver, FIRST).await?;

    for title in ["Move pending", "Skip episode"] {
        let button = driver
            .find_first(&format!(".pending-item [title='{title}']"))
            .await?;
        let fill = button.css("background-color").await?;
        ensure!(
            fill != "rgba(0, 0, 0, 0)" && fill != "transparent",
            "{title} has no fill: {fill}"
        );
    }

    Ok(())
}

/// The schedule entry for the show airing tomorrow.
const FUTURE: &str = ".schedule-item:has([title='Open Future Show'])";

/// Upcoming lists what airs soon: each entry's times sit under its title, and
/// a time opens that episode.
pub async fn upcoming_times_open_their_episode(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    driver.set_window_size(1200, 900).await?;
    driver.reopen_with("view=upcoming").await?;

    driver.find_one_by(FUTURE).await?;
    let title = driver
        .find_one_by(&format!("{FUTURE} .schedule-title"))
        .await?
        .rect()
        .await?;
    let times = driver
        .find_one_by(&format!("{FUTURE} .schedule-times"))
        .await?
        .rect()
        .await?;
    ensure!(
        times.y >= title.y + title.height - 1.0,
        "the times are not under the title: {title:?} {times:?}"
    );
    ensure!(
        driver.count(&format!("{FUTURE} a.schedule-time")).await? == 2,
        "expected both episodes' times"
    );

    driver
        .find_one_by(&format!("{FUTURE} [title='Open S01E02']"))
        .await?
        .click()
        .await?;

    driver.wait_texts(".detail-title", ["Future Show"]).await?;
    let url = driver.webdriver().current_url().await?;
    ensure!(url.fragment() == Some("S01E02"), "the time opened {url}");
    Ok(())
}

/// In the week schedule an entry sits flush left in its day.
pub async fn schedule_entries_sit_flush_left(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(1200, 900).await?;
    driver.reopen_with("view=schedule").await?;
    driver.find_first(".calendar-cell.today").await?;

    // Weeks start on Monday, so on a Sunday tomorrow is in next week.
    let sunday = driver
        .webdriver()
        .execute("return new Date().getDay() === 0;", Vec::new())
        .await?
        .convert::<bool>()?;

    if sunday {
        driver
            .find_one_by("[title='Next week']")
            .await?
            .click()
            .await?;
    }

    let entry = driver.find_one_by(FUTURE).await?.rect().await?;
    let title = driver
        .find_one_by(&format!("{FUTURE} .schedule-title"))
        .await?
        .rect()
        .await?;
    ensure!(
        title.x - entry.x <= 8.0,
        "the title is {}px in from the entry's edge",
        title.x - entry.x
    );
    Ok(())
}

/// Showing another day on Upcoming adds that day and keeps the days already
/// shown, heading and entries alike, instead of rebuilding them.
pub async fn upcoming_keeps_days_when_more_are_shown(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    const DAYS: &str = ".agenda-day";
    const SHOWN: &str = ".agenda-day, .agenda-day *";

    driver.set_window_size(1200, 900).await?;
    driver.reopen_with("view=upcoming").await?;
    driver.find_one_by(FUTURE).await?;

    let days = driver.count(DAYS).await?;
    let marked = mark(driver, SHOWN).await?;

    driver
        .find_one_by("[title='View options']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='More days']")
        .await?
        .click()
        .await?;

    driver.wait_count(DAYS, days + 1).await?;
    driver.wait_count(".agenda-day .skeleton", 0).await?;

    let kept = kept_marked(driver, SHOWN).await?;
    ensure!(
        kept == marked,
        "only {kept} of the {marked} elements of the days shown were kept"
    );
    Ok(())
}

/// Showing another week on the Schedule adds that week and keeps the weeks
/// already shown instead of rebuilding them.
pub async fn schedule_keeps_weeks_when_more_are_shown(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    const WEEKS: &str = ".calendar-week";
    const SHOWN: &str = ".calendar-week, .calendar-week *";

    driver.set_window_size(1200, 900).await?;
    driver.reopen_with("view=schedule").await?;
    driver.find_first(".calendar-week").await?;
    driver.wait_count(".calendar-week .skeleton", 0).await?;

    let weeks = driver.count(WEEKS).await?;
    let marked = mark(driver, SHOWN).await?;

    driver
        .find_one_by("[title='View options']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='More weeks']")
        .await?
        .click()
        .await?;

    driver.wait_count(WEEKS, weeks + 1).await?;
    driver.wait_count(".calendar-week .skeleton", 0).await?;

    let kept = kept_marked(driver, SHOWN).await?;
    ensure!(
        kept == marked,
        "only {kept} of the {marked} elements of the weeks shown were kept"
    );
    Ok(())
}
