use yew_e2e::prelude::*;

use super::Track;

const ESCAPE: &str = "\u{E00C}";
const BACKSPACE: &str = "\u{E003}";

/// Every character in the cast modal, in order.
const CAST: [&str; 8] = [
    "The Duchess",
    "The Narrator",
    "The Codebreaker",
    "The Admiral",
    "The Pathfinder",
    "The Substitute",
    "The Typesetter",
    "The Flight Director",
];

async fn open_cast_modal(driver: &TestDriver) -> Result<()> {
    super::show::open_show(driver).await?;

    driver.find_one_by(".credits-toggle").await?.click().await?;

    driver.find_one_by(".modal.cast-modal").await?;
    Ok(())
}

/// "Show all cast" opens the whole cast in a modal with its search focused,
/// leaving the page's capped grid as it was, and Escape closes it.
pub async fn opens_the_full_cast_in_a_modal(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_cast_modal(driver).await?;

    driver
        .wait_texts(".cast-modal .cast-character", CAST)
        .await?;
    ensure!(
        driver.count(".credits > .cast-grid .cast-card").await? == 6,
        "the page's cast grid grew past its cap"
    );
    ensure!(
        driver.focused_attr("data-test").await?.as_deref() == Some("cast-search"),
        "the cast search is not focused"
    );

    driver
        .find_one_by("[data-test=cast-search]")
        .await?
        .send_keys(ESCAPE)
        .await?;

    driver.wait_count(".modal", 0).await
}

/// The search keeps the cast whose name or character holds every word, and
/// says when nobody matches.
pub async fn search_filters_by_person_and_character(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_cast_modal(driver).await?;

    let search = driver.find_one_by("[data-test=cast-search]").await?;

    search.send_keys("hopper").await?;
    driver
        .wait_texts(".cast-modal .cast-name", ["Grace Hopper"])
        .await?;

    search.send_keys(&BACKSPACE.repeat(6)).await?;
    search.send_keys("the narrator").await?;
    driver
        .wait_texts(".cast-modal .cast-character", ["The Narrator"])
        .await?;

    search.send_keys(" nobody").await?;
    driver.wait_count(".cast-modal .cast-card", 0).await?;
    driver
        .wait_texts(
            "[data-test=cast-empty]",
            ["No cast matches “the narrator nobody”."],
        )
        .await?;

    search.send_keys(&BACKSPACE.repeat(19)).await?;
    driver.wait_texts(".cast-modal .cast-character", CAST).await
}

/// On a phone the cast modal is a full-width sheet whose cards fit across it.
pub async fn phone_cast_modal_fits_the_screen(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    open_cast_modal(driver).await?;
    driver
        .wait_texts(".cast-modal .cast-character", CAST)
        .await?;

    let modal = driver
        .find_one_by(".modal.cast-modal")
        .await?
        .rect()
        .await?;
    let page = driver
        .find_one_by(".modal-background")
        .await?
        .rect()
        .await?;

    ensure!(
        (modal.width - page.width).abs() < 1.0,
        "the modal is {}px wide on a {}px screen",
        modal.width,
        page.width
    );

    let sideways = driver
        .webdriver()
        .execute(
            "const c = document.querySelector('.cast-modal .modal-content'); \
             return c.scrollWidth - c.clientWidth;",
            Vec::new(),
        )
        .await?
        .convert::<f64>()?;
    ensure!(
        sideways <= 1.0,
        "the cast modal scrolls sideways by {sideways}px"
    );

    Ok(())
}
