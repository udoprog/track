use yew_e2e::Fixture;
use yew_e2e::prelude::*;

use super::navigation::wait_heading;
use super::show::focused_title;
use super::{LOGIN_LINK, Track};

const ENTER: &str = "\u{E007}";

/// An administrator adds a user and gets a login link, which the new user
/// opens to choose a password and sign in, as a regular user without the
/// settings and users.
pub async fn creates_a_user_who_registers(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_users(driver).await?;

    let login = driver
        .find_one_by("input[title=\"New user's login\"]")
        .await?;
    login.send_keys("bob").await?;
    driver
        .find_one_by("input[title=\"New user's email\"]")
        .await?
        .send_keys("bob@example.com")
        .await?;
    login.send_keys(ENTER).await?;

    let field = driver
        .find_one_by("input[title='Login link for bob']")
        .await?;
    let url = field.value().await?;
    ensure!(url.contains("/register/"), "the login link is {url:?}");

    driver
        .wait_texts("[data-login=bob] .user-email", ["bob@example.com"])
        .await?;
    let expiry = driver
        .find_one_by("[data-login=bob] [data-test=user-link]")
        .await?
        .text()
        .await?;
    ensure!(
        expiry.starts_with("Link expires"),
        "the link cell says {expiry:?}"
    );

    driver.webdriver().goto(&url).await?;
    driver
        .wait_texts("[data-test=register-login]", ["bob"])
        .await?;
    driver
        .find_one_by("input[title='Password']")
        .await?
        .send_keys("correct horse")
        .await?;
    let confirm = driver
        .find_one_by("input[title='Confirm password']")
        .await?;
    confirm.send_keys("correct horse").await?;
    confirm.send_keys(ENTER).await?;

    driver
        .wait_texts(".toolbar-item[title=Account]", ["bob"])
        .await?;
    hides_admin_pages(driver).await
}

/// A regular user has no settings or users in the app bar, and the users page
/// lists no one for them.
pub async fn regular_users_do_not_see_users(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    let link = format!("{}/register/{LOGIN_LINK}", track.url());
    driver.webdriver().goto(&link).await?;

    for (title, value) in [
        ("Password", "correct horse"),
        ("Confirm password", "correct horse"),
    ] {
        driver
            .find_one_by(&format!("input[title='{title}']"))
            .await?
            .send_keys(value)
            .await?;
    }

    driver
        .find_one_by("input[title='Confirm password']")
        .await?
        .send_keys(ENTER)
        .await?;
    driver
        .wait_texts(".toolbar-item[title=Account]", ["alice"])
        .await?;
    hides_admin_pages(driver).await
}

/// The role is changed from the user's row and stays changed; the
/// administrator's own row can't be demoted or deleted.
pub async fn changes_a_role(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_users(driver).await?;

    driver
        .wait_texts("[data-login=root] .user-role", ["Administrator"])
        .await?;
    ensure!(
        driver.count("[data-login=root] select").await? == 0,
        "your own role can be changed"
    );
    ensure!(
        driver
            .count("[data-login=root] [title='Delete root']")
            .await?
            == 0,
        "you can delete yourself"
    );

    let role = driver.find_one_by("select[title='Role of alice']").await?;
    ensure!(role.value().await? == "regular", "alice is not regular");

    driver
        .find_one_by("select[title='Role of alice'] option[value=admin]")
        .await?
        .click()
        .await?;
    driver
        .wait_texts("[data-login=alice] .field-ok", ["Role changed."])
        .await?;

    driver.reload().await?;
    wait_heading(driver, "Users").await?;
    let role = driver.find_one_by("select[title='Role of alice']").await?;
    ensure!(
        role.value().await? == "admin",
        "alice's new role was not kept"
    );
    Ok(())
}

/// A pending login link shows when it expires and can be revoked, and a user
/// is deleted after confirming.
pub async fn revokes_links_and_deletes(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_users(driver).await?;

    let cell = "[data-login=alice] [data-test=user-link]";
    let expiry = driver.find_one_by(cell).await?.text().await?;
    ensure!(
        expiry.starts_with("Link expires"),
        "the link cell says {expiry:?}"
    );

    driver
        .find_one_by("[title='Revoke login link for alice']")
        .await?
        .click()
        .await?;
    driver.wait_texts(cell, ["No login link"]).await?;

    // From the keyboard, focus moves into the confirmation and back.
    let delete = driver.find_one_by("[title='Delete alice']").await?;
    delete.focus().await?;
    delete.send_keys(ENTER).await?;
    wait_focus(driver, "No").await?;
    driver
        .find_one_by("[data-login=alice] [title=No]")
        .await?
        .send_keys(ENTER)
        .await?;
    wait_focus(driver, "Delete alice").await?;

    driver
        .find_one_by("[title='Delete alice']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[data-login=alice] [title=Yes]")
        .await?
        .click()
        .await?;
    driver.wait_count("[data-login=alice]", 0).await?;

    driver.reload().await?;
    wait_heading(driver, "Users").await?;
    driver.wait_count("[data-test=user]", 1).await?;
    Ok(())
}

async fn wait_focus(driver: &TestDriver, title: &str) -> Result<()> {
    driver
        .wait_until(format_args!("{title:?} to have focus"), async || {
            Ok(focused_title(driver).await? == title)
        })
        .await
}

async fn open_users(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Users]")
        .await?
        .click()
        .await?;
    wait_heading(driver, "Users").await?;
    driver.find_first("[data-test=user]").await?;
    Ok(())
}

async fn hides_admin_pages(driver: &TestDriver) -> Result<()> {
    driver
        .wait_texts(".toolbar-item.active", ["Dashboard"])
        .await?;

    for page in ["Settings", "Users"] {
        ensure!(
            driver
                .count(&format!(".toolbar-item[title={page}]"))
                .await?
                == 0,
            "a regular user sees {page} in the app bar"
        );
    }

    let users = driver.webdriver().current_url().await?.join("/users")?;
    driver.webdriver().goto(users.as_str()).await?;
    driver
        .wait_texts("#page p", ["Only administrators can manage users."])
        .await?;
    ensure!(
        driver.count("[data-test=user]").await? == 0,
        "a regular user sees the users"
    );
    Ok(())
}
