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
