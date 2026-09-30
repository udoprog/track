use yew_e2e::prelude::*;

use super::Track;

/// The Media page is a grid of posters saying what each item is and whether
/// it has been watched.
pub async fn shows_a_poster_grid(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;

    driver.wait_texts(".media-title", ["Seeded Show"]).await?;
    driver
        .find_one_by(".media-card .media-poster image")
        .await?;

    let meta = driver.find_all_texts(".media-card .media-meta").await?;

    ensure!(
        meta.iter().any(|m| m == "2023") && meta.iter().any(|m| m == "Not watched"),
        "expected the year and watch state, got {meta:?}"
    );

    Ok(())
}
