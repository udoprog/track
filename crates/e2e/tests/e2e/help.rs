use yew_e2e::prelude::*;

use super::Track;

const ESCAPE: &str = "\u{E00C}";

/// The sections the help lists, in order.
const SECTIONS: [&str; 5] = [
    "Finding and adding shows and movies",
    "Marking things watched",
    "Remotes and syncing",
    "Other numberings and names",
    "Settings",
];

async fn open_from_toolbar(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by("button.toolbar-item[title=Help]")
        .await?
        .click()
        .await?;

    driver.find_one_by("[data-test=help]").await?;
    Ok(())
}

/// The toolbar opens help at its first section, with the search focused,
/// and Escape closes it.
pub async fn opens_from_the_toolbar(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".site-title", ["Track"]).await?;
    open_from_toolbar(driver).await?;

    driver
        .wait_texts("[data-test=help-title]", [SECTIONS[0]])
        .await?;
    driver.wait_texts("button.help-section", SECTIONS).await?;

    let ret = driver
        .webdriver()
        .execute(
            "return document.activeElement.matches('[data-test=help-search]');",
            Vec::new(),
        )
        .await?;
    ensure!(ret.convert::<bool>()?, "the help search is not focused");

    // Real track components illustrate the text.
    driver
        .find_first("[data-demo=media-kind] button.chip")
        .await?;

    driver
        .find_one_by("[data-test=help-search]")
        .await?
        .send_keys(ESCAPE)
        .await?;

    driver.wait_count("[data-test=help]", 0).await
}

/// Searching keeps only the sections that mention every word, shows the first
/// of them with its hits marked, and says when nothing matches.
pub async fn search_filters_sections(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".site-title", ["Track"]).await?;
    open_from_toolbar(driver).await?;

    let search = driver.find_one_by("[data-test=help-search]").await?;
    search.send_keys("TVmaze").await?;

    driver
        .wait_texts("button.help-section", ["Remotes and syncing"])
        .await?;
    driver
        .wait_texts("[data-test=help-title]", ["Remotes and syncing"])
        .await?;
    ensure!(
        driver.count("mark.help-hit").await? > 0,
        "the hits are not marked"
    );

    search.send_keys(" zzzz").await?;
    driver.wait_count("button.help-section", 0).await?;
    driver.find_one_by("[data-test=help-empty]").await?;

    // Select all and delete, which unlike a WebDriver clear is typed input.
    search.send_keys("\u{E009}a\u{E000}\u{E003}").await?;
    driver
        .wait_count("button.help-section", SECTIONS.len())
        .await
}

/// A link in a section opens the section it names.
pub async fn links_between_sections(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".site-title", ["Track"]).await?;
    open_from_toolbar(driver).await?;

    driver
        .find_one_by("a.help-link[data-section=settings]")
        .await?
        .click()
        .await?;

    driver
        .wait_texts("[data-test=help-title]", ["Settings"])
        .await
}

/// A question mark beside a control opens the section explaining it.
pub async fn inline_help_opens_its_section(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".site-title", ["Track"]).await?;

    let mark = driver.find_one_by("#page button.help-mark").await?;
    ensure!(
        mark.attr("title").await? == "Help: Marking things watched",
        "the dashboard's help mark does not name its section"
    );
    mark.click().await?;

    driver
        .wait_texts("[data-test=help-title]", ["Marking things watched"])
        .await?;

    driver
        .find_one_by(".modal button[title=Close]")
        .await?
        .click()
        .await?;
    driver.wait_count("[data-test=help]", 0).await?;

    super::settings::open_settings(driver).await?;
    super::settings::open_page(driver, "Sources & dates").await?;

    driver
        .find_first(".form-label button.help-mark")
        .await?
        .click()
        .await?;

    driver
        .wait_texts("[data-test=help-title]", ["Remotes and syncing"])
        .await
}

/// On a phone help is a sheet: the sections are a row of chips above the text,
/// and nothing scrolls sideways.
pub async fn phones_stack_the_sections(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(400, 800).await?;
    driver.wait_texts(".site-title", ["Track"]).await?;

    driver
        .find_one_by("#page button.help-mark")
        .await?
        .click()
        .await?;
    driver
        .wait_texts("[data-test=help-title]", ["Marking things watched"])
        .await?;

    let nav = driver.find_one_by(".help-nav").await?.rect().await?;
    let body = driver.find_one_by(".help-body").await?.rect().await?;
    ensure!(
        body.y >= nav.y + nav.height - 1.0,
        "the text is not below the sections: {nav:?} {body:?}"
    );

    let ret = driver
        .webdriver()
        .execute(
            "return [window.innerWidth, document.scrollingElement.scrollWidth];",
            Vec::new(),
        )
        .await?;
    let [width, scrolled] = ret.convert::<[f64; 2]>()?;

    let modal = driver.find_one_by(".modal").await?.rect().await?;
    ensure!(
        modal.x >= 0.0 && modal.x + modal.width <= width,
        "the sheet is wider than the {width}px phone: {modal:?}"
    );
    ensure!(scrolled <= width, "the page scrolls sideways");
    Ok(())
}
