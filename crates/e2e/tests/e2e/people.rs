use yew_e2e::prelude::*;

use super::Track;

/// The people list names everyone, in another language when a person has no
/// name in the display language.
pub async fn lists_people_by_name(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=People]")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".person-name", ["Ada Lovelace", "Greta Garbo"])
        .await
}
