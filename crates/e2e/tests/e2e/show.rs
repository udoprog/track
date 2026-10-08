use yew_e2e::prelude::*;

use super::Track;

/// Open the seeded show from the dashboard.
pub(crate) async fn open_show(driver: &TestDriver) -> Result<()> {
    driver.find_one_by(".pending-title").await?.click().await?;

    driver.wait_texts(".detail-title", ["Seeded Show"]).await
}

/// The show page opens on a heading with the title and first-air year.
pub async fn has_a_heading(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.wait_texts(".detail-meta", ["2023"]).await
}

/// An episode keeps its main actions in view and the rest in its menu.
pub async fn episode_menu_holds_the_other_actions(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_first("[title='More actions']")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            ".menu-list button",
            ["Sync episode", "Translations", "Air dates", "Cache"],
        )
        .await
}

/// Every episode shows its details at once, watched or not: nothing has to be
/// expanded to read it.
pub async fn episodes_show_their_details(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let first = "[id='S01E01']";

    driver
        .find_one_by(&format!("{first} [title='Mark watched']"))
        .await?
        .click()
        .await?;

    driver.wait_count(&format!("{first}.watched"), 1).await?;

    driver
        .wait_until(
            "the watched episode to say when it was watched",
            async || {
                let meta = driver
                    .rendered_texts(&format!("{first} .episode-meta"))
                    .await?;
                Ok(meta.iter().any(|m| m.contains("Watched")))
            },
        )
        .await?;

    ensure!(
        driver.count(".episode .screenshot").await? == driver.count(".episode").await?,
        "an episode is missing its still"
    );
    ensure!(
        driver.count("[title='Show details']").await? == 0,
        "an episode hides its details"
    );
    Ok(())
}

/// The season list says how much of each season has been watched.
pub async fn seasons_count_watched_episodes(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    driver
        .wait_until("the season to read 0/3 watched", async || {
            let texts = driver.find_all_texts(".season-row.current").await?;
            Ok(texts.iter().any(|text| text.contains("0/3")))
        })
        .await
}

/// The episode rail beside the show page gives way on a phone.
pub async fn phones_have_no_episode_rail(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.find_one_by("#outline.visible").await?;

    driver.set_window_size(400, 850).await?;

    let outline = driver.find_one_by("#outline").await?;

    driver
        .wait_until("the episode rail to hide", async || {
            Ok(!outline.visible().await?)
        })
        .await
}

/// On a phone the show page fits the screen: an episode's actions never push
/// past the edge.
pub async fn phones_do_not_scroll_sideways(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.set_window_size(400, 850).await?;
    driver.find_first(".episode [title='Mark watched']").await?;

    let ret = driver
        .webdriver()
        .execute(
            "const e = document.scrollingElement; return [e.scrollWidth, e.clientWidth];",
            Vec::new(),
        )
        .await?;

    let [scroll, client] = ret.convert::<[f64; 2]>()?;
    ensure!(
        scroll <= client,
        "the page is {scroll}px wide on a {client}px screen"
    );
    Ok(())
}

/// On a phone the backdrop shows only behind the heading, not again under the
/// overview.
pub async fn phones_show_the_backdrop_once(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.find_first(".episode [title='Mark watched']").await?;
    driver.snapshot("wide-show-backdrop").await?;

    driver.set_window_size(400, 850).await?;
    driver.find_first(".episode [title='Mark watched']").await?;

    ensure!(
        driver
            .find_one_by(".detail-hero-image")
            .await?
            .visible()
            .await?,
        "the heading has no backdrop"
    );

    let copies = driver
        .rendered_texts(".detail-layout .backdrop")
        .await?
        .len();
    ensure!(
        copies == 0,
        "the backdrop shows {copies} more time(s) on a phone"
    );
    driver.snapshot("phone-show-backdrop").await
}

/// Open the first episode's Translations modal from its menu.
async fn open_episode_modal(driver: &TestDriver) -> Result<()> {
    driver
        .find_first("[title='More actions']")
        .await?
        .click()
        .await?;

    driver
        .find_one_by(".menu-list [title='Translations']")
        .await?
        .click()
        .await?;

    driver.wait_count(".modal", 1).await
}

/// An episode's menu closes when the page around it is clicked.
pub async fn menus_close_on_an_outside_click(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_first("[title='More actions']")
        .await?
        .click()
        .await?;

    // Near the catcher's top-left corner, well away from the menu.
    let catcher = driver.find_one_by(".context-catcher").await?;
    let page = catcher.rect().await?;
    catcher
        .click_by(
            -(page.width / 2.0 - 20.0) as i64,
            -(page.height / 2.0 - 20.0) as i64,
        )
        .await?;

    driver.wait_count(".context-menu", 0).await
}

/// A modal closes from its Close button and from the dimmed page around it.
pub async fn modals_close_from_button_and_backdrop(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

    open_episode_modal(driver).await?;
    driver
        .find_one_by(".modal [title='Close']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal", 0).await?;

    open_episode_modal(driver).await?;
    let modal = driver.find_one_by(".modal").await?.rect().await?;
    let background = driver.find_one_by(".modal-background").await?;
    let page = background.rect().await?;

    // Halfway between the page's top edge and the modal's.
    let dy = (page.y + modal.y) / 2.0 - (page.y + page.height / 2.0);
    background.click_by(0, dy as i64).await?;
    driver.wait_count(".modal", 0).await
}

/// On a phone a modal is a sheet along the bottom edge of the screen.
pub async fn phone_modals_rise_from_the_bottom(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;
    driver.set_window_size(400, 850).await?;
    open_episode_modal(driver).await?;

    let modal = driver.find_one_by(".modal").await?.rect().await?;
    let page = driver
        .find_one_by(".modal-background")
        .await?
        .rect()
        .await?;
    let gap = page.y + page.height - (modal.y + modal.height);

    ensure!(
        gap.abs() < 1.0,
        "the modal ends {gap}px above the bottom of the screen"
    );
    ensure_only_top_border(driver, ".modal").await
}

/// The show's settings are labelled rows whose controls start on one line.
pub async fn settings_line_up_their_controls(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;

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

    // Each row is a group named by its label.
    let row = driver.find_first(".modal .form-row[role=group]").await?;
    let label = driver
        .find_one_by(&format!("#{}", row.attr("aria-labelledby").await?))
        .await?;
    ensure!(label.text().await? == "Language");

    let mut lefts = Vec::new();

    for control in driver.find_all(By::Css(".modal .form-control")).await? {
        lefts.push(control.rect().await?.x);
    }

    ensure!(
        lefts.windows(2).all(|w| (w[0] - w[1]).abs() < 1.0),
        "the controls start at different places: {lefts:?}"
    );
    Ok(())
}

/// The title of whatever has focus.
pub(super) async fn focused_title(driver: &TestDriver) -> Result<String> {
    let ret = driver
        .webdriver()
        .execute("return document.activeElement.title;", Vec::new())
        .await?;
    Ok(ret.convert::<String>()?)
}

/// A modal is a labelled dialog that holds focus: it opens from the keyboard,
/// Tab wraps inside it, and Escape closes it and returns focus to its opener.
pub async fn modals_hold_keyboard_focus(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let settings = driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?;
    settings.focus().await?;
    settings.send_keys("\u{E007}").await?;

    let dialog = driver.find_one_by(".modal[role='dialog']").await?;
    ensure!(dialog.attr("aria-modal").await? == "true");
    let title = driver
        .find_one_by(&format!("#{}", dialog.attr("aria-labelledby").await?))
        .await?;
    ensure!(title.text().await? == "Settings");

    driver
        .wait_until("focus to move into the dialog", async || {
            Ok(driver
                .webdriver()
                .execute(
                    "return !!document.activeElement.closest('.modal');",
                    Vec::new(),
                )
                .await?
                .convert::<bool>()?)
        })
        .await?;

    driver
        .find_one_by(".modal [title='Edit remotes']")
        .await?
        .send_keys("\u{E004}")
        .await?;
    let wrapped = focused_title(driver).await?;
    ensure!(
        wrapped == "Close",
        "Tab from the last control went to {wrapped:?}"
    );

    driver
        .find_one_by(".modal [title='Close']")
        .await?
        .send_keys("\u{E00C}")
        .await?;
    driver.wait_count(".modal", 0).await?;

    let back = focused_title(driver).await?;
    ensure!(back == "Settings", "focus went back to {back:?}");
    Ok(())
}

/// An episode's menu works from the keyboard: Enter opens it on its first
/// item without choosing it, the arrows move between items, and Escape closes
/// it with focus back on the button.
pub async fn menus_work_from_the_keyboard(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let more = driver.find_first("[title='More actions']").await?;
    more.focus().await?;
    more.send_keys("\u{E007}").await?;

    driver.find_one_by("[role='menu']").await?;

    driver
        .wait_until("the first item to take focus", async || {
            Ok(focused_title(driver).await? == "Sync episode")
        })
        .await?;
    ensure!(
        driver.count(".modal").await? == 0,
        "Enter also chose an item"
    );

    driver
        .find_one_by("[role='menuitem'][title='Sync episode']")
        .await?
        .send_keys("\u{E015}")
        .await?;
    let next = focused_title(driver).await?;
    ensure!(next == "Translations", "Down moved to {next:?}");

    driver
        .find_one_by("[role='menuitem'][title='Translations']")
        .await?
        .send_keys("\u{E00C}")
        .await?;
    driver.wait_count("[role='menu']", 0).await?;

    let back = focused_title(driver).await?;
    ensure!(back == "More actions", "focus went back to {back:?}");
    Ok(())
}

/// Mark watched is the same primary button on the dashboard and on the show.
pub async fn mark_watched_is_one_colour(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    let card = driver
        .find_first(".pending-item [title='Mark watched']")
        .await?;
    ensure!(card.attr("class").await?.contains("primary"));

    open_show(driver).await?;

    let episode = driver.find_first(".episode [title='Mark watched']").await?;
    let class = episode.attr("class").await?;
    ensure!(
        class.contains("primary") && !class.contains("success"),
        "the show's Mark watched is {class:?}"
    );
    Ok(())
}

/// On wide screens the seasons are a list that stays beside the episodes:
/// numbered seasons first and specials last, each with its progress bar, and
/// the shown one marked as current.
pub async fn seasons_list_beside_the_episodes(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

    driver
        .wait_texts(
            ".season-list .season-name",
            ["Season 1", "Season 2", "Specials"],
        )
        .await?;

    let list = driver.find_one_by(".season-list").await?;
    ensure!(
        list.css("position").await? == "sticky",
        "the season list scrolls away"
    );
    ensure!(
        driver
            .find_one_by(".season-chips")
            .await?
            .css("display")
            .await?
            == "none",
        "the phone season chips show on a wide screen"
    );
    ensure!(
        driver.count(".season-list .season-progress").await? == 3,
        "not every season shows its progress bar"
    );

    driver
        .wait_texts(
            ".season-list [aria-current=page] .season-name",
            ["Season 1"],
        )
        .await?;

    driver
        .find_one_by(".season-list [title='Show Season 2']")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            ".season-list [aria-current=page] .season-name",
            ["Season 2"],
        )
        .await?;
    driver
        .wait_texts(".detail-content .toolbar h2", ["Season 2"])
        .await
}

/// On a phone the seasons are chips right above the episodes.
pub async fn phones_pick_seasons_from_chips(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.set_window_size(400, 850).await?;

    driver
        .wait_until("the season chips to show instead of the list", async || {
            let chips = driver.find_one_by(".season-chips").await?;
            let list = driver.find_one_by(".season-list").await?;
            Ok(chips.css("display").await? == "flex" && list.css("display").await? == "none")
        })
        .await?;

    ensure!(driver.count(".season-chips button").await? == 3);

    driver
        .find_one_by(".season-chips [title='Show Specials']")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".detail-content .toolbar h2", ["Specials"])
        .await
}

/// On a phone a popover is a sheet along the bottom of the screen, as wide as
/// the screen, wherever the button that opened it is.
pub async fn phone_popovers_are_sheets(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.set_window_size(400, 850).await?;

    driver
        .find_first("[title='More actions']")
        .await?
        .click()
        .await?;

    let menu = driver.find_one_by(".context-menu").await?.rect().await?;
    let page = driver.find_one_by(".context-catcher").await?.rect().await?;

    ensure!(
        (page.y + page.height - (menu.y + menu.height)).abs() < 1.0,
        "the menu does not sit on the bottom edge: {menu:?} in {page:?}"
    );
    ensure!(
        menu.x.abs() < 1.0 && menu.width > page.width - 20.0,
        "the menu is not as wide as the screen: {menu:?} in {page:?}"
    );
    ensure_only_top_border(driver, ".context-menu").await
}

/// A sheet draws only its top edge; side borders would run down the screen
/// edges from its rounded corners.
async fn ensure_only_top_border(driver: &TestDriver, selector: &str) -> Result<()> {
    let ret = driver
        .webdriver()
        .execute(
            "const s = getComputedStyle(document.querySelector(arguments[0])); \
             return [s.borderTopWidth, s.borderRightWidth, s.borderBottomWidth, s.borderLeftWidth].map(parseFloat);",
            vec![selector.into()],
        )
        .await?;

    let [top, right, bottom, left] = ret.convert::<[f64; 4]>()?;
    ensure!(
        top > 0.0 && right == 0.0 && bottom == 0.0 && left == 0.0,
        "{selector} has borders {top} {right} {bottom} {left}"
    );
    Ok(())
}

/// Open the watch history of the episode `code` from its menu.
async fn open_history(driver: &TestDriver, code: &str) -> Result<()> {
    driver
        .find_one_by(&format!("[id='{code}'] [title='More actions']"))
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".menu-list [title='Watch history']")
        .await?
        .click()
        .await?;
    driver.find_one_by(".modal .watch-history").await?;
    Ok(())
}

/// Whether the episode `code` reads `text` in its details.
async fn wait_episode_reads(driver: &TestDriver, code: &str, text: &str) -> Result<()> {
    driver
        .wait_until(format_args!("{code} to read {text:?}"), async || {
            let meta = driver
                .rendered_texts(&format!("[id='{code}'] .episode-meta"))
                .await?;
            Ok(meta.iter().any(|m| {
                m.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains(text)
            }))
        })
        .await
}

/// A watch can be moved to another episode from the watch history, and
/// removed after confirming.
pub async fn watch_history_moves_and_removes(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_one_by("[id='S01E01'] [title='Mark watched']")
        .await?
        .click()
        .await?;
    wait_episode_reads(driver, "S01E01", "Watched once").await?;

    open_history(driver, "S01E01").await?;
    driver
        .find_one_by(".modal [title='Move to another episode']")
        .await?
        .click()
        .await?;

    let episode = driver.find_one_by(".modal select[title=Episode]").await?;
    driver
        .wait_until("the episodes to load", async || {
            Ok(driver.count(".modal select[title=Episode] option").await? == 3)
        })
        .await?;
    driver
        .webdriver()
        .execute(
            "const s = document.querySelector('.modal select[title=Episode]');
             s.value = '2';
             s.dispatchEvent(new Event('change', { bubbles: true }));",
            Vec::new(),
        )
        .await?;
    ensure!(episode.value().await? == "2");

    driver
        .find_one_by(".modal [title='Move the watch here']")
        .await?
        .click()
        .await?;

    // S02 is up next, so its details show that instead of its watches.
    wait_episode_reads(driver, "S01E01", "Never watched").await?;
    driver.wait_count(".modal", 0).await?;

    open_history(driver, "S01E02").await?;
    ensure!(driver.count(".modal .watch-row").await? == 1);

    driver
        .find_one_by(".modal [title=Remove]")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".context-menu [title=Yes]")
        .await?
        .click()
        .await?;

    // With its only watch gone the history has nothing left to show.
    driver.wait_count(".modal .watch-row", 0).await
}

/// The show's tracking toggle is named for a show and says whether it is on.
pub async fn tracking_toggle_names_the_show(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let toggle = driver.find_one_by("[title='Track show']").await?;
    ensure!(toggle.attr("aria-pressed").await? == "true");
    ensure!(driver.count("[title='Track movie']").await? == 0);
    Ok(())
}

/// Closing a page opened from the show's settings goes back to the settings.
pub async fn settings_pages_return_to_settings(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

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
    driver.wait_texts(".modal h2", ["Remotes"]).await?;

    driver
        .find_one_by(".modal [title=Close]")
        .await?
        .click()
        .await?;
    driver.wait_texts(".modal h2", ["Settings"]).await
}

/// The translations modal lists each field's translations with the language
/// beside the text.
pub async fn translations_sit_beside_their_language(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_one_by("[title='Translations']:not(.menu-list *)")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".translations h3", ["Title", "Overview"])
        .await?;

    let language = driver
        .find_first(".translation-language")
        .await?
        .rect()
        .await?;
    let text = driver.find_first(".translation-text").await?.rect().await?;
    ensure!(
        text.x > language.x + language.width && (text.y - language.y).abs() < 8.0,
        "the text is not beside its language: {language:?} {text:?}"
    );
    Ok(())
}

/// The air dates modal names its rule setting, hints quietly that the global
/// default is shared, and needs no warning sign for it.
pub async fn air_dates_explain_the_default_quietly(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

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
        .wait_texts(".modal .form-label", ["Air date rules"])
        .await?;
    driver.find_one_by(".modal .form-row .hint").await?;
    ensure!(
        driver.count(".modal .exclamation-triangle").await? == 0,
        "the default still carries a warning sign"
    );
    Ok(())
}

/// A season's overview offers its other translations, like the show's and
/// the episodes' do.
pub async fn season_overview_switches_language(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

    let season = ".detail-content > .column";
    driver
        .find_one_by(&format!("{season} [title='Change displayed language']"))
        .await?
        .click()
        .await?;

    driver
        .find_one_by(".context-menu [title='Show in Swedish']")
        .await?
        .click()
        .await?;

    driver
        .wait_until("the Swedish overview to show", async || {
            let texts = driver
                .find_all_texts(&format!("{season} .overview"))
                .await?;
            Ok(texts.iter().any(|t| t.contains("Den första säsongen")))
        })
        .await
}

/// The graphics picker labels its actions, marks the current pick, and shows
/// no raw source scores.
pub async fn graphics_say_what_they_do(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal [title='Edit graphics']")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(".modal .gallery-meta .badge", ["Current"])
        .await?;

    for title in ["Pick best poster", "Clear poster"] {
        let text = driver
            .find_one_by(&format!(".modal [title='{title}']"))
            .await?
            .text()
            .await?;
        ensure!(!text.trim().is_empty(), "{title} has no label");
    }

    let meta = driver.rendered_texts(".modal .gallery-meta").await?;
    ensure!(
        meta.iter().all(|m| !m.chars().any(|c| c.is_ascii_digit())),
        "a raw score still shows: {meta:?}"
    );
    Ok(())
}

/// The show's poster beside its seasons is rounded like every other poster.
pub async fn detail_poster_is_rounded(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let radius = driver
        .find_one_by(".detail-sidebar .poster")
        .await?
        .css("border-top-left-radius")
        .await?;
    ensure!(radius == "12px", "the poster's corners are {radius}");
    Ok(())
}

/// A remote identifier added in the Remotes editor shows its source and id on
/// one line with its switch and labelled actions, nothing collapsed.
pub async fn remotes_show_their_actions(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

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

    let switch = driver
        .find_one_by(".modal .remote [title='Enable this remote']")
        .await?
        .text()
        .await?;
    ensure!(switch == "Enabled", "the switch reads {switch:?}");

    for title in ["Edit identifier", "Remove identifier"] {
        let text = driver
            .find_one_by(&format!(".modal .remote [title='{title}']"))
            .await?
            .text()
            .await?;
        ensure!(!text.trim().is_empty(), "{title} has no label");
    }

    ensure!(driver.count("[title='Identifier actions']").await? == 0);
    Ok(())
}

/// XEM, AniDB and scene remotes are added from the Remotes editor, each with
/// its own hint and a text plate for a logo; only AniDB links out, both in the
/// editor and in the show's sources row.
pub async fn xem_anidb_and_scene_remotes_are_added(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

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

    for (source, value, placeholder) in [
        ("anidb", "17617", "Anime id"),
        ("anidb", "18603", "Anime id"),
        ("scene", "Sousou no Frieren", "Scene name"),
        ("xem", "tvdb/424536", "tvdb/<id>"),
    ] {
        driver
            .find_one_by(&format!(
                ".modal select[title=Source] option[value={source}]"
            ))
            .await?
            .click()
            .await?;
        driver.wait_count(".modal .form .hint", 1).await?;

        let input = driver
            .find_one_by(".modal input[aria-label=Identifier]")
            .await?;
        let shown = input.attr("placeholder").await?;
        ensure!(shown == placeholder, "{source} asks for {shown:?}");

        if source == "xem" {
            input.send_keys("424536").await?;
            driver
                .find_one_by(".modal [title='Add identifier']")
                .await?
                .click()
                .await?;
            driver.wait_count(".modal .field-error", 1).await?;
            input.clear().await?;
        }

        input.send_keys(value).await?;
        driver
            .find_one_by(".modal [title='Add identifier']")
            .await?
            .click()
            .await?;
        driver
            .wait_count(
                &format!(".modal .remote .logo.{source}"),
                if value == "18603" { 2 } else { 1 },
            )
            .await?;
    }

    let mut ids = driver.find_all_texts(".modal .remote-id").await?;
    ids.sort();
    ensure!(
        ids == ["17617", "18603", "Sousou no Frieren", "tvdb/424536"],
        "the remotes read {ids:?}"
    );

    ensure!(
        driver
            .count(".modal a.remote-link[href='https://anidb.net/anime/17617'] .logo.anidb")
            .await?
            == 1
    );
    ensure!(driver.count(".modal a.remote-link .logo.scene").await? == 0);
    ensure!(driver.count(".modal a.remote-link .logo.xem").await? == 0);

    for source in ["anidb", "scene", "xem"] {
        let width = driver
            .find_first(&format!(".modal .remote .logo.{source}"))
            .await?
            .css("width")
            .await?;
        let px: f64 = width.trim_end_matches("px").parse()?;
        ensure!(px > 20.0, "the {source} plate is {width} wide");
    }

    for heading in ["Remotes", "Settings"] {
        driver.wait_texts(".modal h2", [heading]).await?;
        driver
            .find_one_by(".modal [title=Close]")
            .await?
            .click()
            .await?;
    }

    driver.wait_count(".modal", 0).await?;

    driver
        .wait_count(
            ".detail-sources a[href='https://anidb.net/anime/18603'] .logo.anidb",
            1,
        )
        .await?;
    ensure!(
        driver
            .count(".detail-sources .logo.scene, .detail-sources .logo.xem")
            .await?
            == 0
    );
    Ok(())
}

/// Clicking stills picks episodes (shift-click picks the range between), and
/// the selection bar marks them all watched at once.
pub async fn picked_episodes_are_marked_together(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

    driver
        .find_one_by("[title='Select S01E01']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[title='Select S01E03']")
        .await?
        .shift_click()
        .await?;

    driver
        .wait_texts(".selection-count", ["3 episodes selected"])
        .await?;
    ensure!(driver.count(".episode-pick[aria-pressed=true]").await? == 3);

    driver
        .find_one_by(".selection-bar [title='Mark the selected episodes watched']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".context-menu [title=Confirm]")
        .await?
        .click()
        .await?;

    driver.wait_count(".selection-bar", 0).await?;

    for code in ["S01E01", "S01E02", "S01E03"] {
        driver
            .wait_until(format_args!("{code} to be watched"), async || {
                Ok(driver.count(&format!("[id='{code}'].watched")).await? == 1)
            })
            .await?;
    }

    Ok(())
}

/// Marking the rest of a season watched moves the pending episode on to the
/// next season, and it stays there after a reload.
pub async fn remaining_episodes_advance_pending(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;
    driver
        .find_one_by("[id='S01E01'] [title='Next episode']")
        .await?;

    driver
        .find_one_by("[title='Mark remaining episodes as watched']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".context-menu [title=Confirm]")
        .await?
        .click()
        .await?;

    for code in ["S01E01", "S01E02", "S01E03"] {
        driver
            .wait_until(format_args!("{code} to be watched"), async || {
                Ok(driver.count(&format!("[id='{code}'].watched")).await? == 1)
            })
            .await?;
    }

    driver.wait_count("[title='Next episode']", 0).await?;

    driver
        .find_one_by(".season-list [title='Show Season 2']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[id='S02E01'] [title='Next episode']")
        .await?;

    driver.reload().await?;
    driver
        .find_one_by("[id='S02E01'] [title='Next episode']")
        .await?;
    ensure!(driver.count("[title='Next episode']").await? == 1);
    Ok(())
}

/// Escape and switching seasons both clear the picked episodes.
pub async fn picked_episodes_clear(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let still = driver.find_one_by("[title='Select S01E01']").await?;
    still.click().await?;
    driver.find_one_by(".selection-bar").await?;

    still.send_keys("\u{E00C}").await?;
    driver.wait_count(".selection-bar", 0).await?;

    driver
        .find_one_by("[title='Select S01E02']")
        .await?
        .click()
        .await?;
    driver.find_one_by(".selection-bar").await?;

    driver
        .find_one_by(".season-list [title='Show Season 2']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(".detail-content .toolbar h2", ["Season 2"])
        .await?;
    ensure!(
        driver.count(".selection-bar").await? == 0,
        "the selection survived the season switch"
    );
    Ok(())
}

/// Expanded phone action menus share icon and label columns.
pub async fn phone_action_rows_align(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.set_window_size(400, 850).await?;
    for title in ["Actions", "Season actions"] {
        driver
            .find_one_by(&format!("button[title='{title}']"))
            .await?
            .click()
            .await?;
    }
    for selector in [
        "#page > .toolbar .toolbar-dropdown > button",
        ".detail-content .toolbar-dropdown > :is(button, a)",
    ] {
        super::navigation::ensure_menu_rows_align(driver, selector).await?;
    }
    driver.snapshot("phone-show-actions").await?;
    driver
        .find_first("[title='More actions']")
        .await?
        .click()
        .await?;
    super::navigation::ensure_menu_rows_align(driver, ".menu-list > button").await?;
    driver.snapshot("phone-episode-actions").await
}

/// A watched episode's rail sample is coloured apart from the pending one.
pub async fn rail_colours_watched_apart_from_pending(
    driver: &mut TestDriver,
    _: &mut Track,
) -> Result<()> {
    open_show(driver).await?;
    driver
        .find_one_by("#outline .outline-sample.pending")
        .await?;

    driver
        .find_first(".episode [title='Mark watched']")
        .await?
        .click()
        .await?;
    driver.find_one_by("#outline .outline-sample.seen").await?;

    let seen = driver
        .find_one_by("#outline .outline-sample.seen")
        .await?
        .css("color")
        .await?;
    let pending = driver
        .find_one_by("#outline .outline-sample.pending")
        .await?
        .css("color")
        .await?;
    ensure!(
        seen != pending,
        "watched and pending samples are both {seen}"
    );
    Ok(())
}

/// A TVmaze id is accepted in the Remotes editor.
pub async fn tvmaze_remote_is_added(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

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
    driver
        .find_one_by(".modal select[title=Source] option[value=tvmaze]")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal input[aria-label=Identifier]")
        .await?
        .send_keys("82")
        .await?;
    driver
        .find_one_by(".modal [title='Add identifier']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .remote .logo.tvmaze", 1).await?;
    ensure!(driver.count(".modal .field-error").await? == 0);
    Ok(())
}
