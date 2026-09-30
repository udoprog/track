use yew_e2e::prelude::*;

use super::Track;

/// On a wide screen the cast sits beside the poster, as on a show, rather
/// than below it.
pub async fn puts_the_cast_beside_the_poster(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(1250, 1000).await?;

    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".media-title", ["Seeded Movie", "Seeded Show"])
        .await?;
    driver
        .find_nth(".media-card .media-poster", 0)
        .await?
        .click()
        .await?;
    driver.wait_texts(".detail-title", ["Seeded Movie"]).await?;

    let layout = driver.find_one_by(".detail-layout").await?.rect().await?;
    let cast = driver.find_one_by(".cast-card").await?.rect().await?;

    ensure!(
        cast.x > layout.x + 100.0,
        "the cast starts at {}px, under the poster column at {}px",
        cast.x,
        layout.x
    );
    Ok(())
}
