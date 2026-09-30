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
        .find_one_by("[data-test=undo]")
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
        .find_one_by("[data-test=confirm-time]")
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
