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

/// On a phone the release date keeps its icon and source logo on a compact
/// line with the date.
pub async fn phone_release_line_stays_together(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    driver.wait_texts(".site-title", ["Track"]).await?;

    driver.find_one_by(".toolbar-toggle").await?.click().await?;
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

    // The header's release date comes first; the watch status reuses the title.
    let release = "indicator[title='Release date']";
    let text = driver
        .find_first(&format!("{release} content > span"))
        .await?
        .rect()
        .await?;
    let icon = driver
        .find_first(&format!("{release} > .item-inline .icon"))
        .await?
        .rect()
        .await?;
    let logo = driver
        .find_first(&format!("{release} .logo"))
        .await?
        .rect()
        .await?;
    let logo_box = driver
        .find_first(&format!("{release} content > .item-inline"))
        .await?
        .rect()
        .await?;

    ensure!(
        logo_box.height <= 32.0,
        "the logo box is {}px tall, a touch target's height",
        logo_box.height
    );

    let text = text.y + text.height / 2.0;
    let icon = icon.y + icon.height / 2.0;
    let logo = logo.y + logo.height / 2.0;

    ensure!(
        (icon - text).abs() < 6.0,
        "the icon sits at {icon}px beside text at {text}px"
    );
    ensure!(
        (logo - text).abs() < 6.0,
        "the logo sits at {logo}px, off the line at {text}px"
    );
    Ok(())
}

/// On a phone the backdrop shows only behind the heading, not again under the
/// overview.
pub async fn phones_show_the_backdrop_once(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
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
    driver.find_one_by(".cast-card").await?;
    driver.snapshot("wide-movie-backdrop").await?;

    driver.set_window_size(400, 850).await?;

    ensure!(
        driver
            .find_one_by(".detail-hero-image")
            .await?
            .visible()
            .await?,
        "the heading has no backdrop"
    );

    let copies = driver
        .rendered_texts(".detail-layout .backdrop")
        .await?
        .len();
    ensure!(
        copies == 0,
        "the backdrop shows {copies} more time(s) on a phone"
    );
    driver.snapshot("phone-movie-backdrop").await
}
