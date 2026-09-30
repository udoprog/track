use yew_e2e::prelude::*;

use super::Track;

/// Opening Search from another page puts the cursor in the search input.
pub async fn focuses_the_input(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".site-title", ["Track"]).await?;

    driver
        .find_one_by(".toolbar-item[title=Search]")
        .await?
        .click()
        .await?;

    driver.wait_texts("#page h1", ["Search Remotes"]).await?;

    let ret = driver
        .webdriver()
        .execute(
            "return document.activeElement.matches('#page input[type=text]');",
            Vec::new(),
        )
        .await?;

    ensure!(ret.convert::<bool>()?, "the search input is not focused");
    Ok(())
}
