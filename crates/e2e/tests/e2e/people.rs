use yew_e2e::prelude::*;

use super::Track;

/// The people list names everyone, in another language when a person has no
/// name in the display language, with the most credited first.
pub async fn lists_people_by_credits(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=People]")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".person-name", ["Greta Garbo", "Ada Lovelace"])
        .await?;

    driver
        .find_one_by("[title='Sort by']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[role=menuitemradio][title='Sort by name']")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".person-name", ["Ada Lovelace", "Greta Garbo"])
        .await?;
    driver.wait_texts("[title='Sort by']", ["Name"]).await
}

/// While the people load, the page shows neither a count nor pages of an
/// empty list.
pub async fn shows_no_count_while_loading(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".site-title", ["Track"]).await?;
    driver.delay_websocket_sends(2000).await?;

    driver
        .find_one_by(".toolbar-item[title=People]")
        .await?
        .click()
        .await?;

    driver.find_one_by("#page .icon.arrow-path.spin").await?;
    ensure!(
        driver.count("#page .list-count").await? == 0,
        "a count shows before the people load"
    );
    ensure!(
        driver.count("#page pagination").await? == 0,
        "pages show before the people load"
    );

    driver.stop_delaying_websocket_sends().await?;
    driver.wait_texts("#page .list-count", ["2 people"]).await
}

/// People without a photo show a silhouette, on the list and in a show's
/// cast, rather than a question mark.
pub async fn shows_a_silhouette_without_a_photo(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    super::show::open_show(driver).await?;
    driver.find_first(".cast-photo .icon.user").await?;

    driver
        .find_one_by(".toolbar-item[title=People]")
        .await?
        .click()
        .await?;

    driver.wait_count(".person-photo .icon.user", 2).await?;
    ensure!(
        driver.count(".icon.question-mark-circle").await? == 0,
        "a person still shows a question mark"
    );
    Ok(())
}

/// On a phone a person's photo keeps its shape beside the name, and the
/// biography runs the width of the page below it.
pub async fn phone_person_page_keeps_the_photo_shape(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    driver.set_window_size(400, 850).await?;
    driver.wait_texts(".site-title", ["Track"]).await?;

    driver.find_one_by(".toolbar-toggle").await?.click().await?;
    driver
        .find_one_by(".toolbar-item[title=People]")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".person-name", ["Greta Garbo", "Ada Lovelace"])
        .await?;
    driver.find_nth(".person-card", 1).await?.click().await?;
    driver.wait_texts("#page h1", ["Ada Lovelace"]).await?;

    let photo = driver
        .find_one_by(".person-detail-photo")
        .await?
        .rect()
        .await?;
    let ratio = photo.height / photo.width;
    ensure!(
        (1.4..=1.6).contains(&ratio),
        "the photo is {}x{}px, not 2:3",
        photo.width,
        photo.height
    );

    let page = driver.find_one_by("#page").await?.rect().await?;
    let bio = driver
        .find_one_by(".person-biography")
        .await?
        .rect()
        .await?;
    ensure!(
        bio.width > page.width * 0.8,
        "the biography is {}px wide on a {}px page",
        bio.width,
        page.width
    );
    Ok(())
}

/// A person with several parts in one show gets one card for it on their
/// page, naming every part.
pub async fn known_for_lists_each_title_once(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=People]")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".person-name", ["Greta Garbo", "Ada Lovelace"])
        .await?;
    driver.find_nth(".person-card", 0).await?.click().await?;

    driver
        .wait_texts(".filmography .person-name", ["Seeded Show"])
        .await?;
    driver
        .wait_texts(
            ".filmography .person-department",
            ["The Duchess, The Narrator"],
        )
        .await
}

/// A person's remotes are added, switched off and removed in the Remotes
/// editor, and the switch holds after the page reloads.
pub async fn edits_a_persons_remotes(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=People]")
        .await?
        .click()
        .await?;
    driver.find_nth(".person-card", 0).await?.click().await?;
    driver.find_one_by(".person-detail-info h1").await?;

    open_remote_editor(driver).await?;

    driver
        .find_one_by(".modal input[aria-label=Identifier]")
        .await?
        .send_keys("12345")
        .await?;
    driver
        .find_one_by(".modal [title='Add identifier']")
        .await?
        .click()
        .await?;
    driver.wait_texts(".modal .remote-id", ["12345"]).await?;

    let switch = ".modal .remote [title='Enable this remote']";
    driver.find_one_by(switch).await?.click().await?;
    driver.wait_texts(switch, ["Disabled"]).await?;

    let url = driver.webdriver().current_url().await?;
    driver.webdriver().goto(url.as_str()).await?;
    driver.find_one_by(".person-detail-info h1").await?;
    open_remote_editor(driver).await?;
    driver.wait_texts(switch, ["Disabled"]).await?;

    driver
        .find_one_by(".modal .remote [title='Remove identifier']")
        .await?
        .click()
        .await?;
    driver.find_one_by("[title='Yes']").await?.click().await?;
    driver.wait_count(".modal .remote", 0).await
}

async fn open_remote_editor(driver: &mut TestDriver) -> Result<()> {
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal [title='Edit remotes']")
        .await?
        .click()
        .await?;
    driver.wait_texts(".modal h2", ["Remotes"]).await
}
