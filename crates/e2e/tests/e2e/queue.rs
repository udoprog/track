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
        .find_one_by("page:not(.current)")
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
        .wait_texts("[title='Show failed tasks']", ["Failed 1"])
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
