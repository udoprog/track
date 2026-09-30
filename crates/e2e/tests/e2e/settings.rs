use yew_e2e::prelude::*;

use super::Track;

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

async fn open_settings(driver: &TestDriver) -> Result<()> {
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
