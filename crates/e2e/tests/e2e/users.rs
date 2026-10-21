use yew_e2e::Fixture;
use yew_e2e::prelude::*;

use super::navigation::wait_heading;
use super::{LOGIN_LINK, Track};

const ENTER: &str = "\u{E007}";

/// An administrator adds a user and gets a registration link, which the new
/// user opens to choose a password and sign in, as a regular user without the
/// users or the system settings.
pub async fn creates_a_user_who_registers(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_users(driver).await?;

    driver.find_one_by(".auth-add").await?.click().await?;
    let login = driver.find_one_by("#auth-new-login").await?;
    login.send_keys("bob").await?;
    driver
        .find_one_by("#auth-new-email")
        .await?
        .send_keys("bob@example.com")
        .await?;
    login.send_keys(ENTER).await?;

    let field = driver.find_one_by("input[title='Link for bob']").await?;
    let url = field.value().await?;
    ensure!(url.contains("/register/"), "the link is {url:?}");

    driver
        .wait_until("bob to be listed as pending", async || {
            let row = driver
                .find_one_by(".auth-user:has([title='Edit bob'])")
                .await?;
            let text = row.text().await?;
            Ok(text.contains("bob@example.com") && text.contains("Pending"))
        })
        .await?;

    driver.webdriver().goto(&url).await?;
    driver
        .wait_texts(".auth-card .auth-value", ["bob", "bob@example.com"])
        .await?;
    driver
        .find_one_by("#auth-password")
        .await?
        .send_keys("correct horse")
        .await?;
    let confirm = driver.find_one_by("#auth-confirm").await?;
    confirm.send_keys("correct horse").await?;
    confirm.send_keys(ENTER).await?;

    driver
        .wait_texts(".toolbar-item[title=Account]", ["bob"])
        .await?;
    hides_admin_pages(driver).await
}

/// A regular user has no users in the app bar, the users page lists no one
/// for them, and their settings hold only their own preferences.
pub async fn regular_users_do_not_see_users(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    register_alice(driver, track).await?;
    hides_admin_pages(driver).await?;

    // Their preferences are their own to change.
    driver
        .find_one_by(".toolbar-item[title=Settings]")
        .await?
        .click()
        .await?;
    let theme = driver.find_one_by("[data-test=theme]").await?;
    driver
        .wait_until("the theme select to read dark", async || {
            Ok(theme.value().await? == "dark")
        })
        .await?;
    driver.set_value(&theme, "light", "change").await?;
    driver.reload().await?;
    let theme = driver.find_one_by("[data-test=theme]").await?;
    driver
        .wait_until("the theme to stay light", async || {
            Ok(theme.value().await? == "light")
        })
        .await
}

/// The role is changed from the user's row and stays changed; the
/// administrator's own row can't be demoted or deleted.
pub async fn changes_a_role(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_users(driver).await?;

    driver
        .wait_texts(".auth-me .auth-user-role", ["Administrator"])
        .await?;
    ensure!(
        driver.count(".auth-me select").await? == 0,
        "your own role can be changed"
    );
    ensure!(
        driver.count("[title='Delete root']").await? == 0,
        "you can delete yourself"
    );

    for (width, name) in [(1280, "users-wide"), (390, "users-phone")] {
        driver.set_window_size(width, 900).await?;
        driver.snapshot(name).await?;
    }
    driver.set_window_size(1280, 900).await?;

    let role = ".auth-user:not(.auth-me) .auth-select";
    ensure!(
        driver.find_one_by(role).await?.value().await? == "regular",
        "alice is not regular"
    );

    driver
        .find_one_by(&format!("{role} option[value=admin]"))
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".auth-admin-users .auth-notice", ["Role changed."])
        .await?;

    driver.reload().await?;
    wait_heading(driver, "Users").await?;
    driver.find_first(".auth-user").await?;
    ensure!(
        driver.find_one_by(role).await?.value().await? == "admin",
        "alice's new role was not kept"
    );
    Ok(())
}

/// A pending link shows when it expires and can be revoked, and a user is
/// deleted after confirming.
pub async fn revokes_links_and_deletes(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_users(driver).await?;

    let cell = ".auth-user:not(.auth-me) .auth-link-state";
    let expiry = driver.find_one_by(cell).await?.text().await?;
    ensure!(
        expiry.starts_with("Link expires"),
        "the link cell says {expiry:?}"
    );

    driver
        .find_one_by("[title='Revoke the link for alice']")
        .await?
        .click()
        .await?;
    driver.wait_texts(cell, ["No link"]).await?;

    // Deleting asks first, and No keeps the user.
    driver
        .find_one_by("[title='Delete alice']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".auth-confirm-no")
        .await?
        .click()
        .await?;
    driver.wait_count(".auth-user", 2).await?;

    driver
        .find_one_by("[title='Delete alice']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".auth-confirm-yes")
        .await?
        .click()
        .await?;
    driver.wait_count(".auth-user", 1).await?;

    driver.reload().await?;
    wait_heading(driver, "Users").await?;
    driver.wait_count(".auth-user", 1).await?;
    Ok(())
}

/// The open users list follows a user added elsewhere, from the live updates
/// on the app's socket, without reloading.
pub async fn follows_users_added_elsewhere(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_users(driver).await?;
    driver.wait_count(".auth-user", 1).await?;

    let status = driver
        .webdriver()
        .execute_async(
            "const done = arguments[arguments.length - 1];
            fetch('/api/auth/users', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ login: 'carol', email: null, role: 'regular' }),
            }).then(r => done(r.status));",
            Vec::new(),
        )
        .await?
        .convert::<u16>()?;
    ensure!(status == 200, "adding carol answered {status}");

    driver.find_one_by("[title='Edit carol']").await?;
    driver.wait_count(".auth-user", 2).await?;
    Ok(())
}

async fn open_users(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Users]")
        .await?
        .click()
        .await?;
    wait_heading(driver, "Users").await?;
    driver.find_first(".auth-user").await?;
    Ok(())
}

/// Only administrators are offered removing a show or movie or deleting a
/// person; a regular user's pages leave the buttons out.
pub async fn regular_users_cannot_remove_media(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    let show = open_media(driver, 1, "Seeded Show").await?;
    driver.find_one_by("[title='Remove show']").await?;

    let movie = open_media(driver, 0, "Seeded Movie").await?;
    driver.find_one_by("[title='Remove movie']").await?;

    driver
        .find_one_by(".toolbar-item[title=People]")
        .await?
        .click()
        .await?;
    driver.find_nth(".person-card", 0).await?.click().await?;
    driver.find_one_by("[title='Delete person']").await?;
    let person = driver.webdriver().current_url().await?;

    register_alice(driver, track).await?;

    driver.webdriver().goto(&show).await?;
    driver.wait_texts(".detail-title", ["Seeded Show"]).await?;
    ensure!(
        driver.count("[title='Remove show']").await? == 0,
        "a regular user is offered Remove show"
    );

    driver.webdriver().goto(&movie).await?;
    driver.wait_texts(".detail-title", ["Seeded Movie"]).await?;
    ensure!(
        driver.count("[title='Remove movie']").await? == 0,
        "a regular user is offered Remove movie"
    );

    driver.webdriver().goto(person.as_str()).await?;
    driver.find_one_by(".person-detail-info h1").await?;
    driver.find_one_by("[title='Sync now']").await?;
    ensure!(
        driver.count("[title='Delete person']").await? == 0,
        "a regular user is offered Delete person"
    );
    Ok(())
}

/// A user made an administrator elsewhere is offered the administrators'
/// actions on the page they have open, without reloading it.
pub async fn admin_actions_follow_a_role_change(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    let show = open_media(driver, 1, "Seeded Show").await?;
    register_alice(driver, track).await?;
    driver.webdriver().goto(&show).await?;
    driver.wait_texts(".detail-title", ["Seeded Show"]).await?;
    ensure!(
        driver.count("[title='Remove show']").await? == 0,
        "a regular user is offered Remove show"
    );

    // localhost keeps its own cookies, so root signs in there beside alice.
    let root = format!("http://localhost:{}/", track.port);

    driver
        .in_second_window(async || {
            driver.webdriver().goto(&root).await?;
            driver
                .wait_texts(".toolbar-item[title=Account]", ["root"])
                .await?;
            open_users(driver).await?;

            driver
                .find_one_by(".auth-user:not(.auth-me) .auth-select option[value=admin]")
                .await?
                .click()
                .await?;
            driver
                .wait_texts(".auth-admin-users .auth-notice", ["Role changed."])
                .await?;
            Ok(())
        })
        .await?;

    driver.find_one_by("[title='Remove show']").await?;
    Ok(())
}

/// Remotes, graphics, sync settings, date rules and the queue are shared by
/// every user, so only administrators are offered changing them.
pub async fn regular_users_cannot_edit_shared_data(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    let movie = open_media(driver, 0, "Seeded Movie").await?;
    let show = open_media(driver, 1, "Seeded Show").await?;

    driver
        .find_one_by(".toolbar-item[title=People]")
        .await?
        .click()
        .await?;
    driver.find_nth(".person-card", 0).await?.click().await?;
    driver.find_one_by(".person-detail-info h1").await?;
    let person = driver.webdriver().current_url().await?;
    let queue = person.join("/queue")?;

    check_shared_controls(driver, &show, &movie, person.as_str(), queue.as_str(), true).await?;
    register_alice(driver, track).await?;
    check_shared_controls(
        driver,
        &show,
        &movie,
        person.as_str(),
        queue.as_str(),
        false,
    )
    .await
}

/// Checks the controls for shared data on each page are offered exactly when
/// `admin` is.
async fn check_shared_controls(
    driver: &TestDriver,
    show: &str,
    movie: &str,
    person: &str,
    queue: &str,
    admin: bool,
) -> Result<()> {
    let present = |selector: &'static str| async move {
        let found = driver.count(selector).await? > 0;
        ensure!(
            found == admin,
            "{selector} offered: {found}, admin: {admin}"
        );
        Ok::<_, anyhow::Error>(())
    };

    driver.webdriver().goto(show).await?;
    driver.find_first("[title='Season Translations']").await?;
    present("[title='Season Graphics']").await?;

    open_settings(driver).await?;
    if admin {
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
    } else {
        driver
            .wait_texts(
                ".modal .form-label",
                ["Language", "Specials", "Last synced"],
            )
            .await?;
    }

    driver.webdriver().goto(show).await?;
    driver
        .find_first("[title='More actions']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".menu-list [title='Air dates']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".modal .text-muted", ["No releases recorded."])
        .await?;
    if admin {
        driver
            .find_one_by(".modal [title='Air date rules']")
            .await?;
    } else {
        present(".modal [title='Air date rules']").await?;
    }

    driver.webdriver().goto(movie).await?;
    driver.wait_texts(".detail-title", ["Seeded Movie"]).await?;
    open_settings(driver).await?;
    if admin {
        driver
            .wait_texts(
                ".modal .form-label",
                [
                    "Language",
                    "Automatic sync",
                    "Release dates",
                    "Last synced",
                    "Graphics",
                    "Remotes",
                ],
            )
            .await?;
    } else {
        driver
            .wait_texts(".modal .form-label", ["Language", "Last synced"])
            .await?;
    }

    driver.webdriver().goto(person).await?;
    driver.find_one_by(".person-detail-info h1").await?;
    open_settings(driver).await?;
    if admin {
        driver
            .wait_texts(".modal .form-label", ["Last synced", "Remotes"])
            .await?;
    } else {
        driver
            .wait_texts(".modal .form-label", ["Last synced"])
            .await?;
    }

    driver.webdriver().goto(queue).await?;
    driver.find_one_by(".queue-now-title").await?;
    present("[title='Queue sync for all show and movies']").await
}

async fn open_settings(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver.find_one_by(".modal").await?;
    Ok(())
}

/// Registers `alice` through the registration link and waits until she is
/// signed in.
async fn register_alice(driver: &TestDriver, track: &Track) -> Result<()> {
    let link = format!("{}/register/{LOGIN_LINK}", track.url());
    driver.webdriver().goto(&link).await?;

    for id in ["auth-password", "auth-confirm"] {
        driver
            .find_one_by(&format!("#{id}"))
            .await?
            .send_keys("correct horse")
            .await?;
    }

    driver
        .find_one_by("#auth-confirm")
        .await?
        .send_keys(ENTER)
        .await?;
    driver
        .wait_texts(".toolbar-item[title=Account]", ["alice"])
        .await?;
    Ok(())
}

/// Opens the `nth` card on the media page and returns the detail page's URL.
async fn open_media(driver: &TestDriver, nth: usize, title: &str) -> Result<String> {
    driver
        .find_one_by(".toolbar-item[title=Media]")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".media-title", ["Seeded Movie", "Seeded Show"])
        .await?;
    driver
        .find_nth(".media-card .media-poster", nth)
        .await?
        .click()
        .await?;
    driver.wait_texts(".detail-title", [title]).await?;
    Ok(driver.webdriver().current_url().await?.to_string())
}

async fn hides_admin_pages(driver: &TestDriver) -> Result<()> {
    driver
        .wait_texts(".toolbar-item.active", ["Dashboard"])
        .await?;

    ensure!(
        driver.count(".toolbar-item[title=Users]").await? == 0,
        "a regular user sees Users in the app bar"
    );

    driver
        .find_one_by(".toolbar-item[title=Settings]")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".settings h2", ["Appearance", "Language"])
        .await?;
    ensure!(
        driver
            .count("input[title='Page title'], #tvdb-api-key, input[title='Team domain']")
            .await?
            == 0,
        "a regular user sees the system settings"
    );

    let users = driver.webdriver().current_url().await?.join("/users")?;
    driver.webdriver().goto(users.as_str()).await?;
    driver
        .wait_texts("#page p", ["Only administrators can manage users."])
        .await?;
    ensure!(
        driver.count(".auth-user").await? == 0,
        "a regular user sees the users"
    );
    Ok(())
}
