use yew_e2e::prelude::*;

use super::Track;

const ENTER: &str = "\u{E007}";

/// Switching the theme restyles the page without a reload, and System leaves
/// the choice to the browser.
pub async fn theme_applies_live(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_settings(driver).await?;
    let dark = background(driver).await?;

    choose_theme(driver, "light").await?;
    wait_theme(driver, "light").await?;

    let light = background(driver).await?;
    ensure!(
        luminance(&light)? > luminance(&dark)?,
        "the light background {light} is not lighter than the dark one {dark}"
    );

    choose_theme(driver, "system").await?;
    wait_theme(driver, "").await?;
    Ok(())
}

/// The theme is saved on the server and applied again after a reload.
pub async fn theme_is_remembered(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_settings(driver).await?;
    choose_theme(driver, "light").await?;
    wait_theme(driver, "light").await?;

    driver.reload().await?;
    wait_theme(driver, "light").await?;

    let select = driver.find_one_by("[data-test=theme]").await?;

    driver
        .wait_until("the theme select to read light", async || {
            Ok(select.value().await? == "light")
        })
        .await?;

    ensure!(
        driver.local_storage("theme").await?.as_deref() == Some("light"),
        "the theme was not kept for the next page load"
    );

    Ok(())
}

/// Fields are drawn by the theme: selects carry their own chevron instead of
/// the browser's, and number fields have no spinner.
pub async fn fields_follow_the_theme(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_settings(driver).await?;

    let select = driver.find_one_by("[data-test=theme]").await?;
    let appearance = select.css("appearance").await?;
    ensure!(
        appearance == "none",
        "the select's appearance is {appearance}"
    );
    let image = select.css("background-image").await?;
    ensure!(
        image.contains("linear-gradient"),
        "the select has no chevron: {image}"
    );

    let number = driver.find_first("input.input-number").await?;
    let appearance = number.css("appearance").await?;
    ensure!(
        appearance == "textfield",
        "the number field's appearance is {appearance}"
    );

    // A focused field shows one ring over its border, not a second outside.
    open_page(driver, "Site").await?;
    let title = driver.find_one_by("input[title='Page title']").await?;
    title.focus().await?;
    let offset = title.css("outline-offset").await?;
    ensure!(offset == "-1px", "the focus ring sits {offset} out");
    Ok(())
}

/// Every toggle on the settings page is a switch the keyboard can reach:
/// Space flips it and it reports its state.
pub async fn switches_work_from_the_keyboard(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_settings(driver).await?;
    open_page(driver, "Sync").await?;

    ensure!(
        driver.count("#content .clickable:not(button, a)").await? == 0,
        "something clickable is neither a button nor a link"
    );

    let switch = driver
        .find_one_by("[role='switch'][title='Automatic sync']")
        .await?;
    let before = switch.attr("aria-checked").await?;
    ensure!(
        before == "true" || before == "false",
        "aria-checked is {before:?}"
    );

    switch.focus().await?;
    switch.send_keys(" ").await?;

    driver
        .wait_until("Space to flip Automatic sync", async || {
            let now = driver
                .find_one_by("[role='switch'][title='Automatic sync']")
                .await?
                .attr("aria-checked")
                .await?;
            Ok(now != before)
        })
        .await
}

/// An administrator sets up Cloudflare Access, and the settings survive a
/// reload.
pub async fn configures_cloudflare_access(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_settings(driver).await?;
    open_page(driver, "Cloudflare Access").await?;

    // The page says where the values come from.
    let intro = driver.find_one_by(".settings-intro").await?.text().await?;
    for needle in [
        "Access → Applications",
        "cloudflareaccess.com",
        "(AUD) Tag",
        "Users page",
    ] {
        ensure!(
            intro.contains(needle),
            "the setup steps do not mention {needle:?}"
        );
    }

    let enabled = "[role='switch'][title='Sign in through Access']";
    driver.find_one_by(enabled).await?.click().await?;

    let domain = driver.find_one_by("input[title='Team domain']").await?;
    domain.send_keys("team.cloudflareaccess.com").await?;
    domain.send_keys(ENTER).await?;

    let audience = driver.find_one_by("input[title='Audience']").await?;
    audience.send_keys("aud-tag").await?;
    audience.send_keys(ENTER).await?;

    driver
        .wait_until("the switch to turn on", async || {
            Ok(driver
                .find_one_by(enabled)
                .await?
                .attr("aria-checked")
                .await?
                == "true")
        })
        .await?;

    driver.reload().await?;

    driver
        .wait_until("the Access settings to come back", async || {
            let domain = driver.find_one_by("input[title='Team domain']").await?;
            let audience = driver.find_one_by("input[title='Audience']").await?;
            let on = driver
                .find_one_by(enabled)
                .await?
                .attr("aria-checked")
                .await?;
            Ok(domain.value().await? == "team.cloudflareaccess.com"
                && audience.value().await? == "aud-tag"
                && on == "true")
        })
        .await
}

/// Trusting the email header without verifying the token is flagged as a risk.
pub async fn warns_about_trusting_only_the_email_header(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_settings(driver).await?;
    open_page(driver, "Cloudflare Access").await?;

    let warning = async || -> Result<Option<String>> {
        match driver
            .find_all(By::Css(".settings .field-error"))
            .await?
            .first()
        {
            Some(warning) => Ok(Some(warning.text().await?)),
            None => Ok(None),
        }
    };

    let switch = async |title: &str, on: bool| -> Result<()> {
        let selector = format!("[role='switch'][title='{title}']");
        driver.find_one_by(&selector).await?.click().await?;
        let want = if on { "true" } else { "false" };

        driver
            .wait_until("the switch to flip", async || {
                Ok(driver
                    .find_one_by(&selector)
                    .await?
                    .attr("aria-checked")
                    .await?
                    == want)
            })
            .await
    };

    switch("Sign in through Access", true).await?;
    switch("Trust email header", true).await?;
    ensure!(warning().await?.is_none(), "warned with the token verified");

    switch("Verify token", false).await?;
    driver
        .wait_until("the warning to show", async || {
            Ok(warning()
                .await?
                .is_some_and(|text| text.contains("sign in as any user")))
        })
        .await?;

    switch("Verify token", true).await?;
    driver
        .wait_until("the warning to go", async || Ok(warning().await?.is_none()))
        .await
}

/// Every setting is a labelled row, and across all sections the controls
/// start on one line.
pub async fn settings_are_labelled_rows(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_settings(driver).await?;

    let pages: [(&str, &[&str]); 6] = [
        ("Preferences", &["Appearance", "Language"]),
        ("Site", &["Site"]),
        ("Sync", &["Sync"]),
        ("Sources & dates", &["Sources & dates"]),
        ("API keys", &["API keys"]),
        ("Cloudflare Access", &["Cloudflare Access"]),
    ];

    let mut lefts = Vec::new();

    for (page, headings) in pages {
        open_page(driver, page).await?;
        match headings {
            [one] => driver.wait_texts(".settings h2", [*one]).await?,
            [one, two] => driver.wait_texts(".settings h2", [*one, *two]).await?,
            _ => bail!("unexpected headings {headings:?}"),
        }

        for control in driver.find_all(By::Css(".settings .form-control")).await? {
            lefts.push(control.rect().await?.x);
        }
    }

    ensure!(lefts.len() > 10, "only {} settings rows", lefts.len());
    ensure!(
        lefts.windows(2).all(|w| (w[0] - w[1]).abs() < 1.0),
        "the controls start at different places: {lefts:?}"
    );
    Ok(())
}

/// On a phone an administrator's settings open on the list of pages; a page
/// replaces the list and leads back to it.
pub async fn phones_list_the_settings_pages(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    driver.find_one_by(".toolbar-toggle").await?.click().await?;
    driver
        .find_one_by(".toolbar-item[title=Settings]")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            ".settings-nav-item",
            [
                "Preferences",
                "Site",
                "Sync",
                "Sources & dates",
                "API keys",
                "Cloudflare Access",
                "Users",
            ],
        )
        .await?;
    ensure!(
        !driver.find_one_by(".settings").await?.visible().await?,
        "a page shows beside the list on a phone"
    );

    driver
        .find_one_by(".settings-nav-item[title='API keys']")
        .await?
        .click()
        .await?;
    driver.wait_texts(".settings h2", ["API keys"]).await?;
    ensure!(
        !driver.find_one_by(".settings-nav").await?.visible().await?,
        "the list stays beside a page on a phone"
    );

    driver
        .find_one_by("[title='All settings']")
        .await?
        .click()
        .await?;
    driver.find_one_by(".settings-nav-item[title=Site]").await?;
    Ok(())
}

/// Tab completes a partly typed time zone, and a second Tab moves on.
pub async fn tab_completes_the_time_zone(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_settings(driver).await?;

    let zone = driver.find_one_by("input[title='Time zone']").await?;
    zone.clear().await?;
    zone.send_keys("europe/sto").await?;
    zone.send_keys("\u{E004}").await?;

    driver
        .wait_until("the zone to complete", async || {
            Ok(zone.value().await? == "Europe/Stockholm")
        })
        .await?;

    let focused = driver
        .webdriver()
        .execute("return document.activeElement.title;", Vec::new())
        .await?
        .convert::<String>()?;
    ensure!(
        focused == "Time zone",
        "focus left the field for {focused:?}"
    );
    Ok(())
}

/// Opens a settings page from the list of pages.
pub(super) async fn open_page(driver: &TestDriver, title: &str) -> Result<()> {
    driver
        .find_one_by(&format!(".settings-nav-item[title='{title}']"))
        .await?
        .click()
        .await?;
    driver
        .wait_until("the page to open", async || {
            let current = driver
                .find_one_by(&format!(".settings-nav-item[title='{title}']"))
                .await?
                .attr("aria-current")
                .await?;
            Ok(current == "page")
        })
        .await
}

pub(super) async fn open_settings(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Settings]")
        .await?
        .click()
        .await?;

    let select = driver.find_one_by("[data-test=theme]").await?;

    // The select is shown before the configuration has loaded into it.
    driver
        .wait_until("the theme select to read dark", async || {
            Ok(select.value().await? == "dark")
        })
        .await
}

async fn choose_theme(driver: &TestDriver, theme: &str) -> Result<()> {
    let select = driver.find_one_by("[data-test=theme]").await?;
    driver.set_value(&select, theme, "change").await
}

/// Wait for the document root to carry `theme`, where empty means none.
async fn wait_theme(driver: &TestDriver, theme: &str) -> Result<()> {
    driver
        .wait_until(format_args!("data-theme to be {theme:?}"), async || {
            let root = driver.find_one_by("html").await?;
            Ok(root.attr("data-theme").await? == theme)
        })
        .await
}

async fn background(driver: &TestDriver) -> Result<String> {
    driver
        .find_one_by("body")
        .await?
        .css("background-color")
        .await
}

/// The mean channel of an `rgb(r, g, b)` color.
fn luminance(color: &str) -> Result<f64> {
    let channels = color
        .trim_start_matches("rgba(")
        .trim_start_matches("rgb(")
        .trim_end_matches(')')
        .split(',')
        .take(3)
        .map(|c| c.trim().parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("reading the color {color}"))?;

    ensure!(channels.len() == 3, "reading the color {color}");
    Ok(channels.iter().sum::<f64>() / 3.0)
}

/// Sync sources are reordered by dragging their handles or with the arrow keys,
/// and the order is saved.
pub async fn reorders_sync_sources(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_settings(driver).await?;
    open_page(driver, "Sources & dates").await?;

    let order = async |driver: &TestDriver| -> Result<Vec<String>> {
        driver.find_all_attrs(".reorder .logo", "class").await
    };

    let before = order(driver).await?;
    ensure!(
        before.len() == 3,
        "expected three sync sources, got {before:?}"
    );

    // From the first handle to the lower half of the second row.
    let first = driver.find_nth(".reorder .drag-handle", 0).await?;
    let from = first.rect().await?;
    let to = driver.find_nth(".reorder > *", 1).await?.rect().await?;
    let dy = (to.y + to.height * 0.75) - (from.y + from.height / 2.0);

    first.drag_by(0, dy as i64).await?;
    driver.drop_held().await?;

    let dragged = vec![before[1].clone(), before[0].clone(), before[2].clone()];

    driver
        .wait_until(
            "the first source to be dragged below the second",
            async || Ok(order(driver).await? == dragged),
        )
        .await?;

    driver.reload().await?;
    driver.find_one_by(".reorder").await?;

    driver
        .wait_until("the dragged order to survive a reload", async || {
            Ok(order(driver).await? == dragged)
        })
        .await?;

    driver
        .press_key_on(".reorder .drag-handle[data-index='2']", "ArrowUp")
        .await?;

    let keyed = vec![dragged[0].clone(), dragged[2].clone(), dragged[1].clone()];

    driver
        .wait_until("the last source to move up with the keyboard", async || {
            Ok(order(driver).await? == keyed)
        })
        .await
}

/// Languages and date rules are added with labelled buttons below their
/// lists, and the rules switch between viewing and editing.
pub async fn adds_languages_and_rules(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_settings(driver).await?;
    open_page(driver, "Sync").await?;

    let add_language = driver.find_one_by("button[title='Add language']").await?;
    ensure!(
        add_language.text().await? == "Add language",
        "Add language has no label"
    );
    add_language.click().await?;
    driver.find_one_by(".modal").await?;
    driver
        .find_one_by(".modal-background")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal", 0).await?;

    open_page(driver, "Sources & dates").await?;
    let rules = driver.count("rule").await?;
    driver
        .find_first("button[title='Add rule']")
        .await?
        .click()
        .await?;
    driver.wait_count("rule", rules + 1).await?;

    driver
        .find_first("button[title='Edit rules']")
        .await?
        .click()
        .await?;
    driver.find_first("button[title='Save rules']").await?;
    Ok(())
}
