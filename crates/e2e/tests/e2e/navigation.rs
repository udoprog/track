use yew_e2e::prelude::*;

use super::Track;

/// The toolbar reaches every top-level page and marks the one showing.
pub async fn opens_every_page(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".site-title", ["Track"]).await?;
    driver
        .wait_texts(".toolbar-item.active", ["Dashboard"])
        .await?;

    for page in ["Media", "People", "Queue"] {
        driver
            .find_one_by(&format!(".toolbar-item[title={page}]"))
            .await?
            .click()
            .await?;

        driver.wait_texts("#page h1", [page]).await?;
        driver.wait_texts(".toolbar-item.active", [page]).await?;
    }

    driver
        .find_one_by(".toolbar-item[title=Settings]")
        .await?
        .click()
        .await?;

    driver.find_one_by("[data-test=theme]").await?;
    driver
        .wait_texts(".toolbar-item.active", ["Settings"])
        .await?;
    Ok(())
}

/// Toolbar icons stay small next to their labels.
pub async fn toolbar_icons_are_small(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".site-title", ["Track"]).await?;

    let icons = driver.find_all(By::Css(".toolbar-item .icon")).await?;
    ensure!(!icons.is_empty(), "the toolbar has no icons");

    for icon in icons {
        let rect = icon.rect().await?;

        ensure!(
            rect.height <= 20.0,
            "a toolbar icon is {}px tall, expected at most 20px",
            rect.height
        );
    }

    Ok(())
}
