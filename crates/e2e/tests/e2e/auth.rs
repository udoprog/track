use yew_e2e::Fixture;
use yew_e2e::prelude::*;

use super::navigation::wait_heading;
use super::{LOGIN_LINK, Track};

const ENTER: &str = "\u{E007}";

/// Signing in from the keyboard with a login name and password opens the app,
/// with the account named in the app bar.
pub async fn signs_in(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    sign_in(driver, "root", "root").await?;
    driver
        .wait_texts(".toolbar-item.active", ["Dashboard"])
        .await?;
    Ok(())
}

/// A wrong password says so and stays on the sign-in page.
pub async fn rejects_a_wrong_password(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    fill_sign_in(driver, "root", "wrong").await?;

    driver
        .wait_texts(".auth-card .field-error", ["Wrong login or password."])
        .await?;

    ensure!(
        driver.count("#toolbar").await? == 0,
        "the app opened after a wrong password"
    );
    Ok(())
}

/// Signing out from the account page returns to the sign-in page, and a reload
/// stays there.
pub async fn signs_out(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    sign_in(driver, "root", "root").await?;
    open_account(driver).await?;

    driver
        .find_one_by("[title='Sign out']")
        .await?
        .click()
        .await?;
    driver.find_one_by("input[title='Login or email']").await?;

    driver.reload().await?;
    driver.find_one_by("input[title='Login or email']").await?;
    ensure!(
        driver.count("#toolbar").await? == 0,
        "the app opened again after signing out"
    );
    Ok(())
}

/// A login link names its account, lets the user choose a password and signs
/// them in; used once, it says it can no longer be used.
pub async fn registers_with_a_login_link(driver: &mut TestDriver, track: &mut Track) -> Result<()> {
    let link = format!("{}/register/{LOGIN_LINK}", track.url());
    driver.webdriver().goto(&link).await?;

    driver
        .wait_texts("[data-test=register-login]", ["alice"])
        .await?;
    driver
        .wait_texts("[data-test=register-email]", ["alice@example.com"])
        .await?;

    let password = driver.find_one_by("input[title='Password']").await?;
    password.send_keys("correct horse").await?;
    let confirm = driver
        .find_one_by("input[title='Confirm password']")
        .await?;
    confirm.send_keys("correct hose").await?;
    confirm.send_keys(ENTER).await?;

    driver
        .wait_texts(".auth-card .field-error", ["The passwords do not match."])
        .await?;

    confirm.clear().await?;
    confirm.send_keys("correct horse").await?;
    confirm.send_keys(ENTER).await?;

    driver
        .wait_texts(".toolbar-item[title=Account]", ["alice"])
        .await?;

    let url = driver.webdriver().current_url().await?;
    ensure!(url.path() == "/", "signed in at {url}");

    driver.webdriver().goto(&link).await?;
    driver
        .wait_texts(
            ".auth-card .field-error",
            ["This login link has already been used or has expired. Ask an administrator for a new one."],
        )
        .await?;
    Ok(())
}

/// The password is changed on the account page, with the current one
/// required, and the new one signs in.
pub async fn changes_the_password(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    sign_in(driver, "root", "root").await?;
    open_account(driver).await?;

    change_password(driver, "nope", "new password", "new password").await?;
    driver
        .wait_texts(
            "form:has([title='Change password']) .field-error",
            ["Server error: The current password is incorrect."],
        )
        .await?;

    change_password(driver, "root", "new password", "new password").await?;
    driver
        .wait_texts(
            "form:has([title='Change password']) .field-ok",
            ["Password changed."],
        )
        .await?;

    driver
        .find_one_by("[title='Sign out']")
        .await?
        .click()
        .await?;

    fill_sign_in(driver, "root", "root").await?;
    driver
        .wait_texts(".auth-card .field-error", ["Wrong login or password."])
        .await?;

    let password = driver.find_one_by("input[title='Password']").await?;
    password.clear().await?;
    password.send_keys("new password").await?;
    password.send_keys(ENTER).await?;
    driver
        .wait_texts(".toolbar-item[title=Account]", ["root"])
        .await?;
    Ok(())
}

/// The account page changes the login, and the app bar follows.
pub async fn changes_the_login(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_account(driver).await?;
    driver
        .wait_texts("[data-test=account-role]", ["Administrator"])
        .await?;

    let login = driver.find_one_by("input[title='Login']").await?;
    login.clear().await?;
    login.send_keys("admin").await?;
    login.send_keys(ENTER).await?;

    driver
        .wait_texts(".toolbar-item[title=Account]", ["admin"])
        .await?;
    driver
        .wait_texts(
            "form:has([title='Save login']) .field-ok",
            ["Login changed."],
        )
        .await?;
    Ok(())
}

async fn fill_sign_in(driver: &TestDriver, login: &str, password: &str) -> Result<()> {
    let field = driver.find_one_by("input[title='Login or email']").await?;
    field.clear().await?;
    field.send_keys(login).await?;

    let field = driver.find_one_by("input[title='Password']").await?;
    field.clear().await?;
    field.send_keys(password).await?;
    field.send_keys(ENTER).await?;
    Ok(())
}

async fn sign_in(driver: &TestDriver, login: &str, password: &str) -> Result<()> {
    fill_sign_in(driver, login, password).await?;
    driver
        .wait_texts(".toolbar-item[title=Account]", [login])
        .await?;
    Ok(())
}

async fn open_account(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Account]")
        .await?
        .click()
        .await?;
    wait_heading(driver, "Account").await
}

async fn change_password(driver: &TestDriver, old: &str, new: &str, confirm: &str) -> Result<()> {
    for (title, value) in [
        ("Current password", old),
        ("New password", new),
        ("Confirm new password", confirm),
    ] {
        let field = driver
            .find_one_by(&format!("input[title='{title}']"))
            .await?;
        field.clear().await?;
        field.send_keys(value).await?;
    }

    driver
        .find_one_by("[title='Change password']")
        .await?
        .click()
        .await?;
    Ok(())
}
