use std::time::Duration;

use yew_e2e::prelude::*;

use super::Track;

const SPINNER: &str = "a[title='Syncing, show in queue'] .icon.spin";
const SYNC: &str = "button[title='Sync now']";

/// Opens the queue and queues a sync of every show, filling more than one page.
async fn sync_all(driver: &mut TestDriver) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Queue]")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='Queue sync for all show and movies']")
        .await?
        .click()
        .await?;
    driver.wait_count(".task-row.pending", 20).await?;
    Ok(())
}

/// Opens the show of the `nth` pending show sync on the queue page shown, and
/// returns that sync's task.
async fn open_pending_show(driver: &mut TestDriver, nth: usize) -> Result<String> {
    let kinds = driver
        .find_all_texts(".task-row.pending .task-kind")
        .await?;
    let ids = driver
        .find_all_attrs(".task-row.pending", "data-task")
        .await?;
    let at = kinds
        .iter()
        .enumerate()
        .filter(|(_, k)| *k == "Show")
        .nth(nth)
        .map(|(i, _)| i)
        .context("too few show syncs are queued")?;
    let id = ids[at].clone();

    driver
        .find_one_by(&format!(".task-row[data-task='{id}'] .task-title"))
        .await?
        .click()
        .await?;
    driver.find_one_by(".detail-title").await?;
    Ok(id)
}

/// The spinner comes from the server's queue, not from what the page saw
/// happen: a reload while the show waits to sync keeps it spinning, until the
/// sync finishes.
pub async fn spinner_survives_a_reload(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    sync_all(driver).await?;
    // The third show waits about 15 seconds behind the queue's delay.
    open_pending_show(driver, 2).await?;
    driver.find_one_by(SPINNER).await?;

    driver.reload().await?;

    driver
        .wait_until_within(
            Duration::from_secs(5),
            "the spinner to spin again",
            async || Ok(driver.count(SPINNER).await? == 1),
        )
        .await?;

    driver
        .wait_until_within(Duration::from_secs(40), "the sync to finish", async || {
            Ok(driver.count(SYNC).await? == 1)
        })
        .await?;

    ensure!(
        driver.count(SPINNER).await? == 0,
        "the spinner still spins after the sync"
    );
    Ok(())
}

/// The spinner is a link to its task in the queue, on whatever page of the
/// queue that task is.
pub async fn spinner_opens_the_task_in_the_queue(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    sync_all(driver).await?;
    driver
        .find_one_by("pagination .page:not(.current)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='Follow the running task']")
        .await?;
    let id = open_pending_show(driver, 0).await?;

    driver.find_one_by(SPINNER).await?;
    driver
        .find_one_by("a[title='Syncing, show in queue']")
        .await?
        .click()
        .await?;

    driver.find_one_by("[data-test=queue-now]").await?;
    driver
        .find_one_by(&format!(".task-row.focused[data-task='{id}']"))
        .await?;
    Ok(())
}
