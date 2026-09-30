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

        wait_heading(driver, page).await?;
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

/// Whether nothing in the page content is clickable without being a button or
/// a link.
async fn only_buttons_and_links_click(driver: &TestDriver) -> Result<()> {
    let stray = driver.count("#content .clickable:not(button, a)").await?;
    ensure!(
        stray == 0,
        "{stray} clickable elements are neither buttons nor links"
    );
    Ok(())
}

/// Going to another page is a real link: it has an address, can be opened in
/// a new tab, and a plain click moves there without reloading the app.
pub async fn navigation_is_links(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    let media = driver.find_one_by("#toolbar a[title='Media']").await?;
    ensure!(media.attr("href").await? == "/media");

    let title = driver.find_one_by("a.pending-title").await?;
    let href = title.attr("href").await?;
    ensure!(
        href.starts_with("/shows/"),
        "the card title links to {href:?}"
    );
    only_buttons_and_links_click(driver).await?;

    driver
        .webdriver()
        .execute("window.__stayed = true;", Vec::new())
        .await?;
    title.click().await?;
    driver.wait_texts(".detail-title", ["Seeded Show"]).await?;

    let stayed = driver
        .webdriver()
        .execute("return window.__stayed === true;", Vec::new())
        .await?
        .convert::<bool>()?;
    ensure!(stayed, "following the link reloaded the app");

    only_buttons_and_links_click(driver).await
}

/// Every page has the landmarks and one heading that name it, for screen
/// readers and agents finding their way around.
pub async fn pages_have_landmarks_and_one_heading(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    for page in ["Dashboard", "Media", "People", "Queue", "Settings"] {
        driver
            .find_one_by(&format!("#toolbar a[title={page}]"))
            .await?
            .click()
            .await?;
        driver
            .wait_texts("#toolbar a[aria-current=page]", [page])
            .await?;

        driver
            .wait_until(format_args!("{page} to have one heading"), async || {
                Ok(driver.count("h1").await? == 1)
            })
            .await?;

        ensure!(
            driver.count("main").await? == 1,
            "{page} has no single main"
        );

        // The app bar names the page, so its heading is only for assistive tech.
        let heading = driver.find_one_by("h1").await?.rect().await?;
        ensure!(heading.width <= 1.0, "{page} shows its heading");
        ensure!(
            driver.count("header#toolbar").await? == 1,
            "{page} has no header"
        );
        ensure!(
            driver.count("nav[aria-label=Main]").await? == 1,
            "{page} has no main navigation"
        );
    }

    Ok(())
}

/// Wait for the page's heading to read `text`. List pages hide it visually
/// (the app bar names the page), so read its text rather than what shows.
pub(crate) async fn wait_heading(driver: &TestDriver, text: &str) -> Result<()> {
    driver
        .wait_until(
            format_args!("the page heading to read {text:?}"),
            async || {
                let heading = driver.find_one_by("#page h1").await?;
                Ok(heading.prop("textContent").await? == text)
            },
        )
        .await
}

/// The app bar's items have room around their labels on a wide screen, and
/// at tablet width they still leave the site title whole.
pub async fn app_bar_items_have_room(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(1250, 900).await?;
    let media = driver.find_one_by("#toolbar a[title=Media]").await?;
    let padding = media.css("padding-left").await?;
    ensure!(padding == "12px", "the items are padded {padding}");

    driver.set_window_size(860, 900).await?;
    let ret = driver
        .webdriver()
        .execute(
            "const t = document.querySelector('.site-title'); return t.scrollWidth <= t.clientWidth + 1;",
            Vec::new(),
        )
        .await?;
    ensure!(ret.convert::<bool>()?, "the site title is clipped at 860px");
    Ok(())
}

/// An error is a card under the app bar: what was being done, then why, and
/// a way to dismiss it. A dropped connection is one way to cause one.
pub async fn errors_show_as_a_card(driver: &mut TestDriver, track: &mut Track) -> Result<()> {
    driver.wait_texts(".site-title", ["Track"]).await?;
    track.child.kill().await?;

    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;

    let card = driver.find_one_by("#error[role=alert]").await?;
    ensure!(
        card.css("position").await? == "fixed",
        "the error pushes the page down"
    );
    driver.find_one_by("#error .error-text strong").await?;

    driver
        .find_one_by("#error [title='Dismiss error']")
        .await?
        .click()
        .await?;
    driver.wait_count("#error", 0).await
}
