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
