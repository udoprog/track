use yew_e2e::prelude::*;

use super::Track;

/// Open the seeded show from the dashboard.
async fn open_show(driver: &TestDriver) -> Result<()> {
    driver.find_one_by(".pending-title").await?.click().await?;

    driver.wait_texts(".detail-title", ["Seeded Show"]).await
}

/// The show page opens on a heading with the title and first-air year.
pub async fn has_a_heading(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.wait_texts(".detail-meta", ["2023"]).await
}

/// An episode keeps its main actions in view and the rest in its menu.
pub async fn episode_menu_holds_the_other_actions(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_first("[title='More actions']")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            ".menu-list button",
            ["Sync episode", "Translations", "Air dates", "Cache"],
        )
        .await
}

/// A watched episode collapses to a compact row, and its disclosure shows the
/// details again.
pub async fn watched_episodes_are_compact(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let first = "[id='S01E01']";
    driver.wait_count(&format!("{first}.compact"), 0).await?;

    driver
        .find_one_by(&format!("{first} [title='Mark watched']"))
        .await?
        .click()
        .await?;

    driver.wait_count(&format!("{first}.compact"), 1).await?;

    driver
        .find_one_by(&format!("{first} [title='Show details']"))
        .await?
        .click()
        .await?;

    driver.wait_count(&format!("{first}.compact"), 0).await
}

/// The season list says how much of each season has been watched.
pub async fn seasons_count_watched_episodes(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    driver
        .wait_until("the season to read 0/3 watched", async || {
            let texts = driver.find_all_texts(".column.active").await?;
            Ok(texts.iter().any(|text| text.contains("0/3")))
        })
        .await
}
