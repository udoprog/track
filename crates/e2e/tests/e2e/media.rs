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

/// The check marks on toggles are icon sized, not as tall as the toggle, on
/// a phone too.
pub async fn toggle_marks_are_icon_sized(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    driver.wait_texts(".site-title", ["Track"]).await?;

    driver.find_one_by(".toolbar-toggle").await?.click().await?;
    driver
        .find_one_by(".toolbar-item[title=Settings]")
        .await?
        .click()
        .await?;

    driver.find_first(".input-checkbox .mark").await?;
    let marks = driver.find_all(By::Css(".input-checkbox .mark")).await?;

    for mark in marks {
        let rect = mark.rect().await?;

        ensure!(
            rect.height <= 20.0,
            "a toggle mark is {}px tall, expected at most 20px",
            rect.height
        );
    }

    Ok(())
}

/// Between phone and full width the controls wrap rather than squeezing the
/// sort label onto two lines and the select to a sliver.
pub async fn sort_stays_readable_at_tablet_width(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    driver.set_window_size(860, 900).await?;

    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;

    let label = driver
        .find_one_by("input-controls .input-label")
        .await?
        .rect()
        .await?;
    let select = driver
        .find_one_by("input-controls select")
        .await?
        .rect()
        .await?;

    ensure!(
        label.height <= 40.0,
        "the sort label wraps to {}px",
        label.height
    );
    ensure!(
        select.width >= 100.0,
        "the sort select is {}px wide",
        select.width
    );
    Ok(())
}
