use yew_e2e::prelude::*;

use super::Track;

/// Open the seeded show from the dashboard.
pub(crate) async fn open_show(driver: &TestDriver) -> Result<()> {
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

/// Every episode shows its details at once, watched or not: nothing has to be
/// expanded to read it.
pub async fn episodes_show_their_details(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let first = "[id='S01E01']";

    driver
        .find_one_by(&format!("{first} [title='Mark watched']"))
        .await?
        .click()
        .await?;

    driver.wait_count(&format!("{first}.watched"), 1).await?;

    driver
        .wait_until(
            "the watched episode to say when it was watched",
            async || {
                let meta = driver
                    .rendered_texts(&format!("{first} .episode-meta"))
                    .await?;
                Ok(meta.iter().any(|m| m.contains("Watched")))
            },
        )
        .await?;

    ensure!(
        driver.count(".episode .screenshot").await? == driver.count(".episode").await?,
        "an episode is missing its still"
    );
    ensure!(
        driver.count("[title='Show details']").await? == 0,
        "an episode hides its details"
    );
    Ok(())
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

/// The episode rail beside the show page gives way on a phone.
pub async fn phones_have_no_episode_rail(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.find_one_by("#outline.visible").await?;

    driver.set_window_size(400, 850).await?;

    let outline = driver.find_one_by("#outline").await?;

    driver
        .wait_until("the episode rail to hide", async || {
            Ok(!outline.visible().await?)
        })
        .await
}

/// On a phone the show page fits the screen: an episode's actions never push
/// past the edge.
pub async fn phones_do_not_scroll_sideways(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.set_window_size(400, 850).await?;
    driver.find_first(".episode [title='Mark watched']").await?;

    let ret = driver
        .webdriver()
        .execute(
            "const e = document.scrollingElement; return [e.scrollWidth, e.clientWidth];",
            Vec::new(),
        )
        .await?;

    let [scroll, client] = ret.convert::<[f64; 2]>()?;
    ensure!(
        scroll <= client,
        "the page is {scroll}px wide on a {client}px screen"
    );
    Ok(())
}

/// Open the first episode's Translations modal from its menu.
async fn open_episode_modal(driver: &TestDriver) -> Result<()> {
    driver
        .find_first("[title='More actions']")
        .await?
        .click()
        .await?;

    driver
        .find_one_by(".menu-list [title='Translations']")
        .await?
        .click()
        .await?;

    driver.wait_count(".modal", 1).await
}

/// An episode's menu closes when the page around it is clicked.
pub async fn menus_close_on_an_outside_click(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_first("[title='More actions']")
        .await?
        .click()
        .await?;

    // Near the catcher's top-left corner, well away from the menu.
    let catcher = driver.find_one_by(".context-catcher").await?;
    let page = catcher.rect().await?;
    catcher
        .click_by(
            -(page.width / 2.0 - 20.0) as i64,
            -(page.height / 2.0 - 20.0) as i64,
        )
        .await?;

    driver.wait_count(".context-menu", 0).await
}

/// A modal closes from its Close button and from the dimmed page around it.
pub async fn modals_close_from_button_and_backdrop(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

    open_episode_modal(driver).await?;
    driver
        .find_one_by(".modal [title='Close']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal", 0).await?;

    open_episode_modal(driver).await?;
    let modal = driver.find_one_by(".modal").await?.rect().await?;
    let background = driver.find_one_by(".modal-background").await?;
    let page = background.rect().await?;

    // Halfway between the page's top edge and the modal's.
    let dy = (page.y + modal.y) / 2.0 - (page.y + page.height / 2.0);
    background.click_by(0, dy as i64).await?;
    driver.wait_count(".modal", 0).await
}

/// On a phone a modal is a sheet along the bottom edge of the screen.
pub async fn phone_modals_rise_from_the_bottom(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;
    driver.set_window_size(400, 850).await?;
    open_episode_modal(driver).await?;

    let modal = driver.find_one_by(".modal").await?.rect().await?;
    let page = driver
        .find_one_by(".modal-background")
        .await?
        .rect()
        .await?;
    let gap = page.y + page.height - (modal.y + modal.height);

    ensure!(
        gap.abs() < 1.0,
        "the modal ends {gap}px above the bottom of the screen"
    );
    Ok(())
}

/// The show's settings are labelled rows whose controls start on one line.
pub async fn settings_line_up_their_controls(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            ".modal .form-label",
            [
                "Language",
                "Automatic sync",
                "Specials",
                "Air dates",
                "Last synced",
                "Graphics",
                "Remotes",
            ],
        )
        .await?;

    let mut lefts = Vec::new();

    for control in driver.find_all(By::Css(".modal .form-control")).await? {
        lefts.push(control.rect().await?.x);
    }

    ensure!(
        lefts.windows(2).all(|w| (w[0] - w[1]).abs() < 1.0),
        "the controls start at different places: {lefts:?}"
    );
    Ok(())
}
