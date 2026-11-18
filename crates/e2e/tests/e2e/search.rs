use yew_e2e::prelude::*;

use super::Track;

/// Opening Search from another page puts the cursor in the search input.
pub async fn focuses_the_input(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver.wait_texts(".track-site-title", ["Track"]).await?;

    driver
        .find_one_by(".track-toolbar-item[title=Search]")
        .await?
        .click()
        .await?;

    super::navigation::wait_heading(driver, "Search").await?;

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

/// Before anything is searched for, the page says what it searches.
pub async fn says_what_it_searches(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".track-toolbar-item[title=Search]")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            "#page p.track-text-muted",
            ["Search TMDB and TVDB for shows and movies to track."],
        )
        .await
}

/// The help mark after the empty page's text sits on the text's last line,
/// centred on it, at desktop and phone widths, and opens the Finding section.
pub async fn help_mark_sits_beside_its_text(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    driver
        .find_one_by(".track-toolbar-item[title=Search]")
        .await?
        .click()
        .await?;

    for width in [1200, 400] {
        driver.set_window_size(width, 800).await?;
        driver
            .find_one_by("#page p.track-text-muted button.track-help-mark")
            .await?;

        let ret = driver
            .webdriver()
            .execute(
                "const mark = document.querySelector('#page p.track-text-muted button.track-help-mark');
                const range = document.createRange();
                range.selectNodeContents(mark.closest('p'));
                range.setEndBefore(mark);
                const lines = [...range.getClientRects()].filter(r => r.width > 0);
                const text = lines[lines.length - 1];
                const button = mark.getBoundingClientRect();
                return [(text.top + text.bottom) / 2, (button.top + button.bottom) / 2, button.left - text.right];",
                Vec::new(),
            )
            .await?;
        let [text, button, gap] = ret.convert::<[f64; 3]>()?;

        ensure!(
            (text - button).abs() <= 2.0,
            "at {width}px the help mark is not centred on its text: {text} vs {button}"
        );
        ensure!(
            (0.0..=16.0).contains(&gap),
            "at {width}px the help mark is not beside its text: {gap}px away"
        );
    }

    driver
        .find_one_by("#page p.track-text-muted button.track-help-mark")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            "[data-test=help-title]",
            ["Finding and adding shows and movies"],
        )
        .await
}
