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

/// Long pages scroll the window itself, with the toolbar staying on top.
pub async fn page_scrolls_the_window(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(1000, 500).await?;

    driver
        .find_one_by(".toolbar-item[title=Settings]")
        .await?
        .click()
        .await?;

    driver.find_one_by("[data-test=theme]").await?;

    let ret = driver
        .webdriver()
        .execute(
            "window.scrollTo(0, document.scrollingElement.scrollHeight); \
             return [window.scrollY, document.getElementById('toolbar').getBoundingClientRect().top];",
            Vec::new(),
        )
        .await?;

    let [scrolled, toolbar] = ret.convert::<[f64; 2]>()?;
    ensure!(scrolled > 0.0, "the window did not scroll");
    ensure!(toolbar == 0.0, "the toolbar scrolled away to {toolbar}px");
    Ok(())
}

/// Every button says what it does, on each page and on a show: they all go
/// through `ui::Button`, which requires a title.
pub async fn every_button_has_a_title(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    let untitled = async |driver: &mut TestDriver, page: &str| -> Result<()> {
        let ret = driver
            .webdriver()
            .execute(
                "return [...document.querySelectorAll('button')] \
                 .filter(b => !b.title).map(b => b.outerHTML.slice(0, 80));",
                Vec::new(),
            )
            .await?;

        let untitled = ret.convert::<Vec<String>>()?;
        ensure!(
            untitled.is_empty(),
            "{page} has untitled buttons: {untitled:?}"
        );
        Ok(())
    };

    super::show::open_show(driver).await?;
    untitled(driver, "The show").await?;

    for page in [
        "Dashboard",
        "Media",
        "People",
        "Search",
        "Queue",
        "Settings",
    ] {
        driver
            .find_one_by(&format!(".toolbar-item[title={page}]"))
            .await?
            .click()
            .await?;

        driver
            .wait_texts(".toolbar-item[aria-current=page]", [page])
            .await?;
        untitled(driver, page).await?;
    }

    Ok(())
}

/// Whatever Tab reaches wears the accent focus ring.
pub async fn tab_shows_a_focus_ring(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    let first = driver.find_one_by("#toolbar [title='Dashboard']").await?;
    first.focus().await?;
    first.send_keys("\u{E004}").await?;

    let ret = driver
        .webdriver()
        .execute(
            "const s = getComputedStyle(document.activeElement);
             const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
             const probe = document.createElement('i');
             probe.style.color = accent;
             document.body.append(probe);
             const color = getComputedStyle(probe).color;
             probe.remove();
             return [document.activeElement.title, s.outlineStyle, s.outlineWidth, s.outlineColor === color];",
            Vec::new(),
        )
        .await?;

    let (title, style, width, accent) = ret.convert::<(String, String, String, bool)>()?;
    ensure!(title == "Media", "Tab moved to {title:?}, not Media");
    ensure!(
        style == "solid" && width == "2px" && accent,
        "the focused button's outline is {style} {width}, accent: {accent}"
    );
    Ok(())
}
