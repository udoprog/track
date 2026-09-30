use yew_e2e::prelude::*;

use super::Track;

/// The queue lists its tasks in aligned columns, without dashes between the
/// fields. A fresh server refreshes its top languages once at start.
pub async fn lists_tasks_in_columns(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".toolbar-item[title=Queue]")
        .await?
        .click()
        .await?;

    driver
        .wait_until("the top languages refresh to complete", async || {
            let kinds = driver.find_all_texts(".task-row .task-kind").await?;
            Ok(kinds.iter().any(|k| k == "Languages"))
        })
        .await?;

    let page = driver.find_one_by("#page").await?.text().await?;
    ensure!(
        !page.contains('—'),
        "the queue still separates fields with dashes"
    );
    Ok(())
}
