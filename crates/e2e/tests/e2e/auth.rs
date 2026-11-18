use yew_e2e::Fixture;
use yew_e2e::prelude::*;

use super::navigation::wait_heading;
use super::{LOGIN_LINK, Track};

const ENTER: &str = "\u{E007}";

pub async fn cloudflare_requires_a_click(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    let selector = "button.auth-cloudflare";
    driver.find_one_by(selector).await?;
    for (width, name) in [(1280, "cloudflare-wide"), (390, "cloudflare-phone")] {
        driver.set_window_size(width, 900).await?;
        driver.find_one_by(selector).await?;
        driver.snapshot(name).await?;
        let fits = driver
            .webdriver()
            .execute(
                "return document.documentElement.scrollWidth <= window.innerWidth;",
                Vec::new(),
            )
            .await?;
        ensure!(fits.convert::<bool>()?, "sign-in overflows at {width}px");
    }
    driver.set_window_size(1280, 900).await?;
    ensure!(
        driver.count("#toolbar").await? == 0,
        "Cloudflare signed in automatically"
    );
    driver.reload().await?;
    driver.find_one_by(selector).await?;
    driver.find_one_by("#auth-password").await?;
    driver.find_one_by(selector).await?.click().await?;
    driver
        .wait_texts(".track-toolbar-item[title=Account]", ["root"])
        .await?;
    driver.reload().await?;
    driver
        .wait_texts(".track-toolbar-item[title=Account]", ["root"])
        .await?;
    open_account(driver).await?;
    driver.find_one_by(".auth-sign-out").await?.click().await?;
    driver.find_one_by(selector).await?;
    driver.reload().await?;
    driver.find_one_by(selector).await?;
    ensure!(
        driver.count("#toolbar").await? == 0,
        "Cloudflare signed in after logout"
    );
    fill_sign_in(driver, "root", "wrong").await?;
    driver
        .wait_texts(".auth-card .auth-error", ["Wrong login or password."])
        .await?;
    ensure!(
        driver.count("#toolbar").await? == 0,
        "Enter selected Cloudflare instead of password sign-in"
    );
    sign_in(driver, "root", "root").await?;
    Ok(())
}

pub async fn cloudflare_is_unavailable(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.find_one_by("#auth-login").await?;
    let available = driver.webdriver().execute_async(
        "const done = arguments[arguments.length - 1]; fetch('/api/auth/config').then(r => r.json()).then(c => done(c.cloudflare));",
        Vec::new(),
    ).await?;
    ensure!(
        !available.convert::<bool>()?,
        "Cloudflare unexpectedly available"
    );
    ensure!(
        driver.count("button.auth-cloudflare").await? == 0,
        "unavailable Cloudflare button shown"
    );
    Ok(())
}

/// Signing in from the keyboard with a login name and password opens the app,
/// with the account named in the app bar.
pub async fn signs_in(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    sign_in(driver, "root", "root").await?;
    driver
        .wait_texts(".track-toolbar-item.track-active", ["Dashboard"])
        .await?;
    Ok(())
}

/// A wrong password says so and stays on the sign-in page.
pub async fn rejects_a_wrong_password(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    fill_sign_in(driver, "root", "wrong").await?;

    driver
        .wait_texts(".auth-card .auth-error", ["Wrong login or password."])
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

    driver.find_one_by(".auth-sign-out").await?.click().await?;
    driver.find_one_by("#auth-login").await?;

    driver.reload().await?;
    driver.find_one_by("#auth-login").await?;
    ensure!(
        driver.count("#toolbar").await? == 0,
        "the app opened again after signing out"
    );
    Ok(())
}

/// A registration link names its account, lets the user choose a password and
/// signs them in; used once, it says it can no longer be used.
pub async fn registers_with_a_login_link(driver: &mut TestDriver, track: &mut Track) -> Result<()> {
    let link = format!("{}/register/{LOGIN_LINK}", track.url());
    driver.webdriver().goto(&link).await?;

    driver
        .wait_texts(".auth-card .auth-value", ["alice", "alice@example.com"])
        .await?;

    let password = driver.find_one_by("#auth-password").await?;
    password.send_keys("correct horse").await?;
    let confirm = driver.find_one_by("#auth-confirm").await?;
    confirm.send_keys("correct hose").await?;
    confirm.send_keys(ENTER).await?;

    driver
        .wait_texts(".auth-card .auth-error", ["The passwords do not match."])
        .await?;

    confirm.clear().await?;
    confirm.send_keys("correct horse").await?;
    confirm.send_keys(ENTER).await?;

    driver
        .wait_texts(".track-toolbar-item[title=Account]", ["alice"])
        .await?;

    let url = driver.webdriver().current_url().await?;
    ensure!(url.path() == "/", "signed in at {url}");

    driver.webdriver().goto(&link).await?;
    driver
        .wait_texts(
            ".auth-card .auth-error",
            ["This link has already been used or has expired. Ask an administrator for a new one."],
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
            ".auth-password .auth-error",
            ["The current password is incorrect."],
        )
        .await?;

    change_password(driver, "root", "new password", "new password").await?;
    driver
        .wait_texts(
            ".auth-password .auth-notice",
            ["Password changed. Your other sessions were signed out."],
        )
        .await?;

    driver.find_one_by(".auth-sign-out").await?.click().await?;

    fill_sign_in(driver, "root", "root").await?;
    driver
        .wait_texts(".auth-card .auth-error", ["Wrong login or password."])
        .await?;

    let password = driver.find_one_by("#auth-password").await?;
    password.clear().await?;
    password.send_keys("new password").await?;
    password.send_keys(ENTER).await?;
    driver
        .wait_texts(".track-toolbar-item[title=Account]", ["root"])
        .await?;
    Ok(())
}

/// The account page changes the login, and the app bar follows.
pub async fn changes_the_login(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_account(driver).await?;
    driver
        .wait_texts(".auth-details .auth-value", ["Administrator", "Password"])
        .await?;
    driver.snapshot("account").await?;

    let login = driver.find_one_by("#auth-account-login").await?;
    login.clear().await?;
    login.send_keys("admin").await?;
    login.send_keys(ENTER).await?;

    driver
        .wait_texts(".track-toolbar-item[title=Account]", ["admin"])
        .await?;
    driver
        .wait_texts(".auth-profile .auth-notice", ["Saved."])
        .await?;
    Ok(())
}

async fn fill_sign_in(driver: &TestDriver, login: &str, password: &str) -> Result<()> {
    let field = driver.find_one_by("#auth-login").await?;
    field.clear().await?;
    field.send_keys(login).await?;

    let field = driver.find_one_by("#auth-password").await?;
    field.clear().await?;
    field.send_keys(password).await?;
    field.send_keys(ENTER).await?;
    Ok(())
}

async fn sign_in(driver: &TestDriver, login: &str, password: &str) -> Result<()> {
    fill_sign_in(driver, login, password).await?;
    driver
        .wait_texts(".track-toolbar-item[title=Account]", [login])
        .await?;
    Ok(())
}

async fn open_account(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by(".track-toolbar-item[title=Account]")
        .await?
        .click()
        .await?;
    wait_heading(driver, "Account").await
}

async fn change_password(driver: &TestDriver, old: &str, new: &str, confirm: &str) -> Result<()> {
    for (id, value) in [
        ("auth-old-password", old),
        ("auth-new-password", new),
        ("auth-confirm-password", confirm),
    ] {
        let field = driver.find_one_by(&format!("#{id}")).await?;
        field.clear().await?;
        field.send_keys(value).await?;
    }

    driver
        .find_one_by(".auth-password .auth-submit")
        .await?
        .click()
        .await?;
    Ok(())
}
