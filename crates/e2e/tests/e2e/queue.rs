use std::time::Duration;

use yew_e2e::prelude::*;

use super::Track;

/// Long enough for a queued task to wait out the queue's 5 second delay and run.
const TASK_RUNS: Duration = Duration::from_secs(20);

async fn open_queue(driver: &mut TestDriver) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Queue]")
        .await?
        .click()
        .await?;

    driver.find_one_by("[data-test=queue-now]").await?;
    Ok(())
}

async fn sync_all(driver: &mut TestDriver) -> Result<()> {
    driver
        .find_one_by("[title='Queue sync for all show and movies']")
        .await?
        .click()
        .await
}

/// The queue lists its tasks in aligned columns, without dashes between the
/// fields. A fresh server refreshes its top languages once at start.
pub async fn lists_tasks_in_columns(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_queue(driver).await?;

    driver
        .wait_until("the top languages refresh to complete", async || {
            let kinds = driver.find_all_texts(".task-row.done .task-kind").await?;
            Ok(kinds.iter().any(|k| k == "Languages"))
        })
        .await?;

    let page = driver.find_one_by("#page").await?.text().await?;
    ensure!(
        !page.contains('—'),
        "the queue still separates fields with dashes"
    );

    // On a wide screen a task's kind and subject sit side by side.
    let kind = driver
        .find_first(".task-row .task-kind")
        .await?
        .rect()
        .await?;
    let title = driver
        .find_first(".task-row .task-title")
        .await?
        .rect()
        .await?;
    let (kind_mid, title_mid) = (kind.y + kind.height / 2.0, title.y + title.height / 2.0);
    ensure!(
        (kind_mid - title_mid).abs() < 4.0 && title.x > kind.x,
        "the task's subject is not beside its kind: {kind:?} {title:?}"
    );
    Ok(())
}

/// A task keeps its row while it goes from pending to run, and the strip
/// above the timeline keeps its height throughout.
pub async fn keeps_rows_in_place(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_queue(driver).await?;
    let strip = driver.find_one_by("[data-test=queue-now]").await?;
    let height = strip.rect().await?.height;

    sync_all(driver).await?;

    let row = driver.find_first(".task-row.pending[data-task]").await?;
    let id = row.attr("data-task").await?;
    let ids = driver.find_all_attrs(".task-row", "data-task").await?;
    let index = ids.iter().position(|i| *i == id);

    ensure!(
        strip.rect().await?.height == height,
        "the strip changed height when a task was queued"
    );

    let selector = format!(".task-row[data-task='{id}']");

    driver
        .wait_until_within(TASK_RUNS, "the queued task to run", async || {
            let class = driver.find_one_by(&selector).await?.attr("class").await?;

            Ok(class.contains("done") || class.contains("failed"))
        })
        .await?;

    let ids = driver.find_all_attrs(".task-row", "data-task").await?;
    ensure!(
        ids.iter().position(|i| *i == id) == index,
        "the task moved when it ran: {ids:?}"
    );
    ensure!(
        strip.rect().await?.height == height,
        "the strip changed height when a task ran"
    );
    Ok(())
}

/// Paging away stops following the queue; Follow brings back the page with
/// the next task on it.
pub async fn follows_the_next_task(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_queue(driver).await?;
    sync_all(driver).await?;

    driver.wait_count(".task-row.pending", 20).await?;
    ensure!(
        driver.count("[title='Follow the running task']").await? == 0,
        "the queue starts out not following"
    );

    driver
        .find_one_by("pagination .page:not(.current)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='Follow the running task']")
        .await?
        .click()
        .await?;

    driver
        .wait_count("[title='Follow the running task']", 0)
        .await?;

    let next = driver
        .find_one_by("[data-test=queue-now] .task-title")
        .await?
        .text()
        .await?;

    let titles = driver.find_all_texts(".task-row .task-title").await?;
    ensure!(
        titles.contains(&next),
        "the next task {next:?} is not on the followed page: {titles:?}"
    );
    Ok(())
}

/// A task that fails shows as failed, with its error, and the Failed filter
/// lists it. Removing a show while its sync waits makes that sync fail.
pub async fn shows_failed_tasks(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_queue(driver).await?;
    sync_all(driver).await?;

    driver.find_first(".task-row.pending").await?;
    let kinds = driver
        .find_all_texts(".task-row.pending .task-kind")
        .await?;
    let ids = driver
        .find_all_attrs(".task-row.pending", "data-task")
        .await?;
    let at = kinds
        .iter()
        .position(|k| k == "Show")
        .context("no show sync was queued")?;
    let id = ids[at].clone();
    let row = format!(".task-row[data-task='{id}']");

    driver
        .find_one_by(&format!("{row} .task-title"))
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='Remove show']")
        .await?
        .click()
        .await?;
    driver.find_one_by("[title=Yes]").await?.click().await?;

    open_queue(driver).await?;

    driver
        .wait_until_within(
            TASK_RUNS,
            "the sync of the removed show to fail",
            async || {
                let class = driver.find_one_by(&row).await?.attr("class").await?;
                Ok(class.contains("failed"))
            },
        )
        .await?;

    driver
        .find_one_by("[title='Show failed tasks']")
        .await?
        .click()
        .await?;

    driver
        .wait_texts("[title='Show failed tasks'] .chip-count", ["1"])
        .await?;
    driver.wait_count(".task-row", 1).await?;

    let error = driver
        .find_one_by(&format!("{row} .task-error"))
        .await?
        .text()
        .await?;
    ensure!(
        error.contains("Expected show to exist"),
        "the failed task says {error:?}"
    );
    Ok(())
}

/// An empty filter says, across the list, what it has nothing of.
pub async fn empty_filters_say_what_is_missing(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_queue(driver).await?;

    driver
        .find_one_by("[title='Show failed tasks']")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".task-empty", ["No syncs have failed."])
        .await?;

    let line = driver.find_one_by(".task-empty").await?.rect().await?;
    let list = driver.find_one_by(".task-timeline").await?.rect().await?;
    let middle = list.x + list.width / 2.0;
    ensure!(
        (line.x + line.width / 2.0 - middle).abs() < 2.0,
        "the line is not centred in the list: {line:?} in {list:?}"
    );
    Ok(())
}

/// The status card's determinate bar, while the running task has one.
const STRIP_BAR: &str = "[data-test=queue-now] .task-progress[aria-valuenow]";

/// Queue a sync of the seeded show and wait until it fetches translations
/// from the slow TMDB stand-in, returning how far its bar is.
async fn sync_until_translations(driver: &mut TestDriver) -> Result<u32> {
    sync_all(driver).await?;

    driver
        .wait_until_within(TASK_RUNS, "the sync to fetch translations", async || {
            let steps = driver
                .find_all_texts("[data-test=queue-now] .queue-now-step-label")
                .await?;
            Ok(steps.iter().any(|s| s == "TMDB translations")
                && driver.count(STRIP_BAR).await? == 1)
        })
        .await?;

    let bar = driver.find_one_by(STRIP_BAR).await?;
    // One request for the show, one per season and one per episode.
    ensure!(
        bar.attr("aria-valuemax").await? == "19",
        "the bar does not count the translation requests"
    );
    Ok(bar.attr("aria-valuenow").await?.parse()?)
}

/// Wait until the status card's bar is past halfway and past `first`, or
/// gone with its step.
async fn wait_for_advance(driver: &mut TestDriver, first: u32) -> Result<()> {
    driver
        .wait_until_within(TASK_RUNS, "the bar to pass halfway", async || {
            let Ok(bar) = driver.find_one_by(STRIP_BAR).await else {
                return Ok(true);
            };
            let now = bar.attr("aria-valuenow").await?.parse::<u32>()?;
            Ok(now > first && now >= 10)
        })
        .await
}

/// A running show sync says what it is doing, with a bar that fills as it
/// goes, in the status card and on its row; both go once it finishes, and the
/// card keeps its height throughout.
pub async fn shows_sync_progress(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_queue(driver).await?;
    let strip = driver.find_one_by("[data-test=queue-now]").await?;
    let height = strip.rect().await?.height;

    let first = sync_until_translations(driver).await?;

    let count = driver
        .find_one_by("[data-test=queue-now] .queue-now-step-count")
        .await?
        .text()
        .await?;
    ensure!(
        count.ends_with("of 19 requests"),
        "the card counts {count:?}"
    );

    let step = driver
        .find_one_by(".task-row.running .task-step")
        .await?
        .text()
        .await?;
    ensure!(
        step.starts_with("TMDB translations"),
        "the running row says {step:?}"
    );
    ensure!(
        driver
            .count(".task-row.running .task-progress[aria-valuenow]")
            .await?
            == 1,
        "the running row has no bar"
    );
    ensure!(
        strip.rect().await?.height == height,
        "the status card changed height while showing progress"
    );

    wait_for_advance(driver, first).await?;
    driver.snapshot("queue-sync-progress").await?;

    driver
        .wait_until_within(TASK_RUNS, "the sync to finish", async || {
            Ok(driver.count(".task-row.running").await? == 0)
        })
        .await?;

    let kinds = driver.find_all_texts(".task-row.done .task-kind").await?;
    ensure!(
        kinds.iter().any(|k| k == "Show"),
        "the sync did not finish cleanly: {kinds:?}"
    );
    ensure!(
        driver.count(".task-progress").await? == 0 && driver.count(".queue-now-step").await? == 0,
        "progress is still shown after the sync finished"
    );
    ensure!(
        strip.rect().await?.height == height,
        "the status card changed height when the sync finished"
    );
    Ok(())
}

/// On a phone the status card carries the step inside its own bounds, and the
/// running row keeps only its bar.
pub async fn phone_shows_sync_progress(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_queue(driver).await?;
    driver.set_window_size(400, 850).await?;
    let strip = driver.find_one_by("[data-test=queue-now]").await?;
    let height = strip.rect().await?.height;

    let first = sync_until_translations(driver).await?;
    wait_for_advance(driver, first).await?;
    driver.snapshot("phone-queue-sync-progress").await?;

    let card = strip.rect().await?;
    let count = driver
        .find_one_by("[data-test=queue-now] .queue-now-step-count")
        .await?
        .rect()
        .await?;
    ensure!(
        card.height == height
            && count.x >= card.x
            && count.x + count.width <= card.x + card.width
            && count.y + count.height <= card.y + card.height,
        "the step count leaves the card: {count:?} {card:?}"
    );

    let step = driver
        .find_one_by(".task-row.running .task-step")
        .await?
        .rect()
        .await?;
    ensure!(
        step.width == 0.0,
        "the running row repeats the step on a phone"
    );
    ensure!(
        driver.count(".task-row.running .task-progress").await? == 1,
        "the running row has no bar"
    );
    Ok(())
}
