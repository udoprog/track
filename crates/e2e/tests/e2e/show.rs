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
        .wait_texts("[data-testid='detail-content'] .toolbar h2", ["Season 2"])
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
        .wait_texts("[data-testid='detail-content'] .toolbar h2", ["Specials"])
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

/// The show's tracking toggle is named for a show and its title says whether
/// it is on, following the state as it is toggled.
pub async fn tracking_toggle_names_the_show(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let toggle = driver.find_one_by("[title='Tracking show']").await?;
    ensure!(toggle.attr("aria-pressed").await? == "true");
    ensure!(driver.count("[title$='movie']").await? == 0);
    toggle.click().await?;

    driver
        .wait_count("[title='Not tracking show'][aria-pressed='false']", 1)
        .await?;
    ensure!(driver.count("[title='Tracking show']").await? == 0);
    driver
        .find_one_by("[title='Not tracking show']")
        .await?
        .click()
        .await?;

    driver
        .wait_count("[title='Tracking show'][aria-pressed='true']", 1)
        .await
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

    let season = "[data-testid='detail-content'] > .column";
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

    driver
        .wait_texts(".modal [data-testid='remote-id']", ["12345"])
        .await?;

    let switch = driver
        .find_one_by(".modal [data-testid='remote-actions'] [role='switch']")
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

    let mut ids = driver
        .find_all_texts(".modal [data-testid='remote-id']")
        .await?;
    ids.sort();
    ensure!(
        ids == ["17617", "18603", "Sousou no Frieren", "tvdb/424536"],
        "the remotes read {ids:?}"
    );

    ensure!(
        driver
            .count(".modal a[data-testid='remote-link'][href='https://anidb.net/anime/17617'] .logo.anidb")
            .await?
            == 1
    );
    ensure!(
        driver
            .count(".modal a[data-testid='remote-link'] .logo.scene")
            .await?
            == 0
    );
    ensure!(
        driver
            .count(".modal a[data-testid='remote-link'] .logo.xem")
            .await?
            == 0
    );

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
        .find_one_by("[id='S01E01'] .item-inline[title='Next episode']")
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

    driver
        .wait_count(".item-inline[title='Next episode']", 0)
        .await?;

    driver
        .find_one_by(".season-list [title='Show Season 2']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("[id='S02E01'] .item-inline[title='Next episode']")
        .await?;

    driver.reload().await?;
    driver
        .find_one_by("[id='S02E01'] .item-inline[title='Next episode']")
        .await?;
    ensure!(driver.count(".item-inline[title='Next episode']").await? == 1);
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
        .wait_texts("[data-testid='detail-content'] .toolbar h2", ["Season 2"])
        .await?;
    ensure!(
        driver.count(".selection-bar").await? == 0,
        "the selection survived the season switch"
    );
    Ok(())
}

/// Switching from a season with neither XEM season names nor an overview to
/// one with names keeps the page working, as from Bleach's first season to its
/// second.
pub async fn season_names_appear_on_switching(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    track.execute(
        "DELETE FROM xem_names WHERE show_id = 1001 AND season = 1;
        DELETE FROM season_strings WHERE season_id = 2001;",
    )?;

    open_show(driver).await?;
    driver.find_one_by("#S01E05").await?;

    driver
        .find_one_by(".season-list [title='Show Season 2']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts("[data-testid='detail-content'] .toolbar h2", ["Season 2"])
        .await?;
    driver.find_one_by("#S02E02").await?;
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
        "[data-testid='detail-content'] .toolbar-dropdown > :is(button, a)",
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

/// The value of every element `selector` matches, in order.
/// Wait for the listed ranges' spans, each side read as `S1 E1–E3`. Their
/// numbers are cells of a grid, which visible text would split into lines.
async fn wait_spans(driver: &TestDriver, expected: &[&str]) -> Result<()> {
    driver
        .wait_until(format!("the ranges to read {expected:?}"), async || {
            let ret = driver
                .webdriver()
                .execute(
                    "return [...document.querySelectorAll('.modal .numbering-range-summary .numbering-span')].map(e => e.textContent);",
                    Vec::new(),
                )
                .await?;
            Ok(ret.convert::<Vec<String>>()? == expected)
        })
        .await
}

async fn values(driver: &TestDriver, selector: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();

    for e in driver.find_all(By::Css(selector)).await? {
        out.push(e.prop("value").await?);
    }

    Ok(out)
}

/// Click the editor's episode named `label`, such as `TMDB S1 E4`.
async fn pick(driver: &TestDriver, label: &str) -> Result<()> {
    driver
        .find_one_by(&format!(".modal .numbering-ep[aria-label='{label}']"))
        .await?
        .click()
        .await
}

/// Answer the editor's question of which numbering to suggest from, after
/// checking it offers TheTVDB with the numberings that agree with it, and
/// Trakt.
async fn suggest_from(driver: &TestDriver, label: &str) -> Result<()> {
    driver
        .wait_texts(
            ".modal .numbering-basis-name",
            ["TheTVDB · Scene · AniDB", "Trakt"],
        )
        .await?;
    driver
        .wait_texts(
            ".modal .numbering-basis-option .text-muted",
            ["5 episodes in 2 ranges", "4 episodes in 1 range"],
        )
        .await?;
    driver
        .find_one_by(&format!(".modal [title='Suggest from {label}']"))
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-basis", 0).await
}

/// A show whose TMDB season doesn't line up with XEM warns in automatic
/// mode, and the range editor shows where: TMDB episodes beside every
/// numbering XEM has, with bands for each range. A suggestion previews there unsaved; ranges are
/// selected, respanned, removed and created by clicking episodes, an overlap
/// is flagged, and automatic mode clears the ranges again.
pub async fn numbering_ranges_are_edited(driver: &mut TestDriver, track: &mut Track) -> Result<()> {
    let numbering =
        || track.query("SELECT COALESCE(numbering, 'automatic') FROM shows WHERE id = 1001");

    open_show(driver).await?;
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            ".modal .numbering-warning",
            ["TMDB Season 1 has 5 episodes but TheTVDB Season 1 has 3, so from S01E04 on, other numberings would be wrong. Compare in the range editor"],
        )
        .await?;

    // Automatic numbering, drawn: TheTVDB S1 has no E4-E5 and S2 is unreached.
    driver
        .find_one_by(".modal .numbering-warning .link-button")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal .numbering-details-toggle")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(
            ".modal .numbering-status .numbering-missing",
            ["XEM has no TheTVDB S1 E4–E5, so TMDB S1 E4–E5 get no other numbers."],
        )
        .await?;
    driver
        .wait_texts(
            ".modal .numbering-unreached",
            ["No TMDB episode maps to TheTVDB S2 E1–E2."],
        )
        .await?;
    ensure!(driver.count(".modal .numbering-ep.missing").await? == 2);
    ensure!(driver.count(".modal .numbering-ep:not(:disabled)").await? == 0);

    // Trakt orders the episodes differently from the others, so suggesting
    // asks which to follow; the suggestion shows in the same view before
    // anything is saved.
    driver
        .find_one_by(".modal .numbering-status [title='Suggest from episode order']")
        .await?
        .click()
        .await?;
    suggest_from(driver, "TheTVDB").await?;
    wait_spans(driver, &["S1 E1–E3", "S1 E1–E3", "S1 E4–", "S2 E1–"]).await?;
    // Two ranges, each banded across the four gaps between five columns.
    driver
        .wait_count(".modal .numbering-band:not(.open-end)", 8)
        .await?;
    driver.wait_count(".modal .numbering-note", 1).await?;
    driver
        .wait_texts(
            ".modal .numbering-coverage",
            ["5 of 5 regular TMDB episodes are mapped."],
        )
        .await?;
    ensure!(numbering()? == ["automatic"], "suggesting saved");

    // A mapped episode selects its range; two clicks respan it.
    pick(driver, "TMDB S1 E4").await?;
    driver
        .wait_count(".modal .numbering-range.selected[aria-label='Range 2']", 1)
        .await?;
    let fields = ".modal .numbering-range.selected input";
    ensure!(values(driver, fields).await? == ["1", "4", "", "2", "1"]);

    pick(driver, "TMDB S1 E4").await?;
    pick(driver, "TMDB S1 E3").await?;
    driver
        .wait_texts(
            ".modal .numbering-errors",
            ["E3 is already in the range S1 E1–E3. TMDB ranges must not overlap."],
        )
        .await?;
    driver
        .wait_count(".modal .numbering-band.invalid", 4)
        .await?;
    driver
        .wait_count(".modal .numbering-ep.overlap[aria-label='TMDB S1 E3']", 1)
        .await?;
    driver
        .wait_count(".modal [title='Save']:disabled", 1)
        .await?;

    pick(driver, "TMDB S1 E4").await?;
    pick(driver, "TMDB S1 E5").await?;
    driver.wait_count(".modal .numbering-errors", 0).await?;
    ensure!(values(driver, fields).await? == ["1", "4", "5", "2", "1"]);

    // Removing it leaves its episodes unmapped.
    driver
        .find_one_by(".modal [title='Remove range 2']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-range", 1).await?;
    driver
        .wait_count(".modal .numbering-ep.unmapped", 2)
        .await?;
    driver
        .wait_texts(
            ".modal .numbering-coverage",
            ["3 of 5 regular TMDB episodes are mapped."],
        )
        .await?;
    driver
        .wait_texts(".modal .numbering-unmapped", ["Not mapped: S1 E4–E5."])
        .await?;

    // An unmapped episode starts a range that continues the one before it,
    // which runs past what XEM knows until it is pointed at TheTVDB S2.
    pick(driver, "TMDB S1 E4").await?;
    driver.wait_count(".modal .numbering-range", 2).await?;
    pick(driver, "TMDB S1 E5").await?;
    driver
        .wait_texts(
            ".modal .numbering-range .numbering-missing",
            ["XEM has no TheTVDB S1 E4–E5."],
        )
        .await?;
    ensure!(values(driver, fields).await? == ["1", "4", "5", "1", "4"]);

    pick(driver, "TheTVDB S2 E1").await?;
    driver.wait_count(".modal .numbering-missing", 0).await?;
    ensure!(values(driver, fields).await? == ["1", "4", "5", "2", "1"]);
    wait_spans(driver, &["S1 E1–E3", "S1 E1–E3", "S1 E4–E5", "S2 E1–E2"]).await?;

    driver
        .find_one_by(".modal [title='Save']")
        .await?
        .click()
        .await?;
    driver.find_one_by(".modal [title='Edit ranges']").await?;

    let saved = numbering()?;
    ensure!(
        saved
            == [
                r#"{"ranges":[{"season":1,"first":1,"last":3,"system":"tvdb","target_season":1,"target_first":1},{"season":1,"first":4,"last":5,"system":"tvdb","target_season":2,"target_first":1}]}"#
            ],
        "saved {saved:?}"
    );

    // The editor opens on the saved ranges, and saving automatic clears them.
    driver
        .find_one_by(".modal [title='Edit ranges']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-range", 2).await?;
    driver
        .find_one_by(".modal [title='Automatic: same as TheTVDB']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-range", 0).await?;
    driver
        .find_one_by(".modal [title='Save']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-warning", 1).await?;
    driver
        .wait_until("the ranges to be cleared", async || {
            Ok(numbering()? == ["automatic"])
        })
        .await
}

/// Every numbering XEM has gets a column, also on a phone; switching to
/// manual ranges asks which to suggest from since they disagree, and ranges
/// are made by clicking another numbering's episodes as well as TMDB's, each
/// reading the same from either side.
pub async fn numbering_ranges_are_made_from_either_side(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    open_show(driver).await?;
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal .numbering-warning .link-button")
        .await?
        .click()
        .await?;

    driver.wait_count(".modal .numbering-map-source", 5).await?;
    ensure!(
        driver
            .find_all_attrs(".modal .numbering-map-source", "title")
            .await?
            == ["TMDB", "TheTVDB", "Scene", "AniDB", "Trakt"]
    );

    driver
        .find_one_by(".modal [title='Manual ranges']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-basis", 1).await?;
    ensure!(driver.count(".modal .numbering-range").await? == 0);

    suggest_from(driver, "Trakt").await?;
    wait_spans(driver, &["S1 E1–E4", "S1 E1–E4"]).await?;
    driver
        .wait_texts(
            ".modal .numbering-note",
            ["Suggested from Trakt's episode order. Nothing is saved until you save."],
        )
        .await?;
    driver
        .find_one_by(".modal .numbering-details-toggle")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(
            ".modal .numbering-unreached",
            ["No TMDB episode maps to TheTVDB S1 E1."],
        )
        .await?;

    // Two clicks in TheTVDB's column make a range spanning them, from the
    // first TMDB episode no range covers.
    driver
        .find_one_by(".modal [title='Remove range 1']")
        .await?
        .click()
        .await?;
    pick(driver, "TheTVDB S1 E1").await?;
    pick(driver, "TheTVDB S1 E3").await?;
    let fields = ".modal .numbering-range.selected input";
    ensure!(values(driver, fields).await? == ["1", "1", "3", "1", "1"]);

    driver
        .find_one_by(".modal [title='Done editing range 1']")
        .await?
        .click()
        .await?;
    pick(driver, "TheTVDB S2 E1").await?;
    pick(driver, "TheTVDB S2 E2").await?;
    wait_spans(driver, &["S1 E1–E3", "S1 E1–E3", "S1 E4–E5", "S2 E1–E2"]).await?;
    driver
        .wait_texts(
            ".modal .numbering-coverage",
            ["5 of 5 regular TMDB episodes are mapped."],
        )
        .await?;

    // Each side names the other, and AniDB, reached through XEM, too.
    let title = async |label: &str| {
        driver
            .find_one_by(&format!(".modal .numbering-ep[aria-label='{label}']"))
            .await?
            .attr("title")
            .await
    };
    ensure!(title("TMDB S1 E4").await? == "TMDB S1 E4 ↔ TheTVDB S2 E1");
    ensure!(title("TheTVDB S2 E1").await? == "TheTVDB S2 E1 ↔ TMDB S1 E4");
    ensure!(title("AniDB S1 E4").await? == "AniDB S1 E4 ↔ TMDB S1 E4");

    // An episode reached through XEM selects its range.
    driver
        .find_one_by(".modal [title='Done editing range 2']")
        .await?
        .click()
        .await?;
    pick(driver, "Scene S2 E2").await?;
    driver
        .wait_count(".modal .numbering-range.selected[aria-label='Range 2']", 1)
        .await?;

    // Five columns fit a phone without the modal scrolling sideways.
    driver.set_window_size(400, 850).await?;
    driver.wait_count(".modal .numbering-map-source", 5).await?;
    let ret = driver
        .webdriver()
        .execute(
            "const e = document.querySelector('.modal .numbering-map-scroll'); return [e.scrollWidth, e.clientWidth];",
            Vec::new(),
        )
        .await?;
    let [scroll, client] = ret.convert::<[f64; 2]>()?;
    ensure!(scroll <= client, "the map is {scroll}px wide in {client}px");

    driver
        .find_one_by(".modal [title='Save']")
        .await?
        .click()
        .await?;
    driver
        .wait_until("the ranges to be saved", async || {
            Ok(track.query("SELECT numbering FROM shows WHERE id = 1001")?
                == [r#"{"ranges":[{"season":1,"first":1,"last":3,"system":"tvdb","target_season":1,"target_first":1},{"season":1,"first":4,"last":5,"system":"tvdb","target_season":2,"target_first":1}]}"#])
        })
        .await
}

/// Automatic mode folds its warnings into a one-line summary with Suggest
/// right under it, and manual mode folds what no range maps the same way.
pub async fn numbering_warnings_fold_into_a_summary(
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
        .find_one_by(".modal .numbering-warning .link-button")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            ".modal .numbering-summary",
            ["1 season differs from TheTVDB. Show details"],
        )
        .await?;
    ensure!(
        driver
            .count(
                ".modal .numbering-summary + .numbering-tools [title='Suggest from episode order']"
            )
            .await?
            == 1
    );
    ensure!(driver.count(".modal .numbering-details").await? == 0);

    // The details unfold under Suggest, which stays where it was.
    let toggle = ".modal .numbering-details-toggle";
    driver.find_one_by(toggle).await?.click().await?;
    driver
        .wait_texts(
            ".modal .numbering-details .numbering-warning",
            ["TMDB Season 1 has 5 episodes but TheTVDB Season 1 has 3, so from S01E04 on, other numberings would be wrong."],
        )
        .await?;
    driver
        .wait_texts(
            ".modal .numbering-details .numbering-missing",
            ["XEM has no TheTVDB S1 E4–E5, so TMDB S1 E4–E5 get no other numbers."],
        )
        .await?;
    driver
        .wait_texts(
            ".modal .numbering-details .numbering-unreached",
            ["No TMDB episode maps to TheTVDB S2 E1–E2."],
        )
        .await?;
    driver
        .wait_count(".modal .numbering-tools + .numbering-details", 1)
        .await?;
    driver.wait_texts(toggle, ["Hide details"]).await?;
    ensure!(
        driver
            .find_one_by(toggle)
            .await?
            .attr("aria-expanded")
            .await?
            == "true"
    );

    driver.find_one_by(toggle).await?.click().await?;
    driver.wait_count(".modal .numbering-details", 0).await?;

    // Manual ranges fold the episodes no range maps under their count.
    driver
        .find_one_by(".modal [title='Manual ranges']")
        .await?
        .click()
        .await?;
    suggest_from(driver, "TheTVDB").await?;
    driver
        .find_one_by(".modal [title='Remove range 2']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(
            ".modal .numbering-coverage",
            ["3 of 5 regular TMDB episodes are mapped."],
        )
        .await?;
    ensure!(driver.count(".modal .numbering-unmapped").await? == 0);

    driver.find_one_by(toggle).await?.click().await?;
    driver
        .wait_texts(
            ".modal .numbering-details .numbering-unmapped",
            ["Not mapped: S1 E4–E5."],
        )
        .await
}

/// A show whose seasons line up with TheTVDB's opens the read-only map from
/// its settings without switching to manual ranges.
pub async fn numbering_map_opens_when_seasons_line_up(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    track.execute("DELETE FROM episodes WHERE id IN (3004, 3005);")?;

    open_show(driver).await?;
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal .numbering-view-map")
        .await?
        .click()
        .await?;

    driver
        .wait_texts(
            ".modal .numbering-status .hint",
            ["Automatic numbering reads the show's episode codes as TheTVDB's, and its seasons line up."],
        )
        .await?;
    driver.wait_count(".modal .numbering-map-source", 5).await?;
    ensure!(
        driver
            .count(".modal [title='Automatic: same as TheTVDB'][aria-pressed='true']")
            .await?
            == 1
    );
    ensure!(driver.count(".modal .numbering-range").await? == 0);
    Ok(())
}

/// Whether the element `selector` matches lies within `scroller`, its closest
/// ancestor matching it, or with no scroller within every scrolling ancestor
/// and the window.
async fn in_view(driver: &TestDriver, selector: &str, scroller: Option<&str>) -> Result<bool> {
    let script = format!(
        r#"
        const e = document.querySelector({selector:?});
        if (!e) return false;
        const r = e.getBoundingClientRect();
        const within = (q) => r.top >= q.top - 1 && r.bottom <= q.bottom + 1 && r.left >= q.left - 1 && r.right <= q.right + 1;
        const scroller = {scroller};
        if (scroller) return within(e.closest(scroller).getBoundingClientRect());
        for (let p = e.parentElement; p; p = p.parentElement) {{
            const s = getComputedStyle(p);
            if (/auto|scroll|hidden/.test(s.overflowX + s.overflowY) && !within(p.getBoundingClientRect())) return false;
        }}
        return within({{ top: 0, left: 0, bottom: innerHeight, right: innerWidth }});
        "#,
        scroller = scroller.map_or("null".to_owned(), |s| format!("{s:?}")),
    );

    let ret = driver.webdriver().execute(&script, Vec::new()).await?;
    Ok(ret.convert::<bool>()?)
}

/// Selecting a range in the list scrolls the map to its episodes, and
/// clicking an episode in the map scrolls the list to its range, at wide and
/// phone widths.
pub async fn numbering_list_and_map_follow_each_other(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    // Thirty more seasons of three episodes, a range each.
    track.execute(
        "INSERT INTO episodes (id, show_id, season, episode, aired)
         WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 89)
         SELECT 4000 + i, 1001, 2 + i / 3, 1 + i % 3, 1710000000000 + i * 604800000 FROM n;
         INSERT INTO xem_episodes (show_id, entry, system, part, season, episode, absolute)
         WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 89)
         SELECT 1001, 100 + i, 'tvdb', 0, 3 + i / 3, 1 + i % 3, 100 + i FROM n;",
    )?;

    let ranges = (2..=31)
        .map(|s| {
            format!(
                r#"{{"season":{s},"first":1,"last":3,"system":"tvdb","target_season":{},"target_first":1}}"#,
                s + 1
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    track.execute(&format!(
        r#"UPDATE shows SET numbering = '{{"ranges":[{{"season":1,"first":1,"last":3,"system":"tvdb","target_season":1,"target_first":1}},{{"season":1,"first":4,"last":5,"system":"tvdb","target_season":2,"target_first":1}},{ranges}]}}' WHERE id = 1001"#
    ))?;

    open_show(driver).await?;

    let last = ".modal .numbering-range[aria-label='Range 32']";
    let first_ep = ".modal .numbering-ep[aria-label='TMDB S31 E1']";
    let target_ep = ".modal .numbering-ep[aria-label='TheTVDB S32 E1']";

    let top = async || {
        driver
            .webdriver()
            .execute(
                "for (const e of document.querySelectorAll('.modal, .modal *')) { e.scrollTop = 0; }",
                Vec::new(),
            )
            .await
    };

    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal [title='Edit ranges']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-range", 32).await?;
    driver.find_one_by(first_ep).await?;

    for (width, height) in [(1400, 900), (400, 850)] {
        driver.set_window_size(width, height).await?;
        top().await?;

        // The list picks the last range: the map scrolls to its episodes.
        ensure!(!in_view(driver, first_ep, Some(".numbering-map-scroll")).await?);
        driver
            .find_one_by(".modal [title='Edit range 32']")
            .await?
            .click()
            .await?;
        driver.wait_count(&format!("{last}.selected"), 1).await?;
        ensure!(
            in_view(driver, first_ep, Some(".numbering-map-scroll")).await?,
            "the map did not scroll to TMDB S31 E1 at {width}px"
        );
        ensure!(
            in_view(driver, target_ep, Some(".numbering-map-scroll")).await?,
            "the map did not scroll to TheTVDB S32 E1 at {width}px"
        );

        // Back at the top of both, a click on the last episode in the map
        // scrolls the list to its range.
        driver
            .find_one_by(".modal [title='Done editing range 32']")
            .await?
            .click()
            .await?;
        driver
            .wait_count(".modal .numbering-range.selected", 0)
            .await?;
        top().await?;
        ensure!(!in_view(driver, last, None).await?);
        driver
            .find_one_by(".modal .numbering-ep[aria-label='TMDB S31 E3']")
            .await?
            .click()
            .await?;
        driver.wait_count(&format!("{last}.selected"), 1).await?;
        ensure!(
            in_view(driver, last, None).await?,
            "the list did not scroll to range 32 at {width}px"
        );

        driver
            .find_one_by(".modal [title='Done editing range 32']")
            .await?
            .click()
            .await?;
        driver
            .wait_count(".modal .numbering-range.selected", 0)
            .await?;
    }

    Ok(())
}

/// The map's head names the season at the top of each column as the map
/// scrolls, a dash past a column's last, and picking a season there scrolls
/// the map to it, at wide and phone widths.
pub async fn numbering_map_names_the_season_in_view(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    // Thirty more seasons of three episodes, on TMDB and TheTVDB.
    track.execute(
        r#"INSERT INTO episodes (id, show_id, season, episode, aired)
         WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 89)
         SELECT 4000 + i, 1001, 2 + i / 3, 1 + i % 3, 1710000000000 + i * 604800000 FROM n;
         INSERT INTO xem_episodes (show_id, entry, system, part, season, episode, absolute)
         WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 89)
         SELECT 1001, 100 + i, 'tvdb', 0, 3 + i / 3, 1 + i % 3, 100 + i FROM n;
         UPDATE shows SET numbering = '{"ranges":[{"season":1,"first":1,"last":3,"system":"tvdb","target_season":1,"target_first":1}]}' WHERE id = 1001;"#,
    )?;

    open_show(driver).await?;
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal [title='Edit ranges']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal .numbering-ep[aria-label='TMDB S31 E1']")
        .await?;

    let checked = |label: &str, season: &str| {
        format!(
            ".modal select[aria-label='{label} season in view'] option[value='{season}']:checked"
        )
    };

    for (width, height) in [(1400, 900), (400, 850)] {
        driver.set_window_size(width, height).await?;
        driver
            .webdriver()
            .execute(
                "for (const e of document.querySelectorAll('.modal, .modal *')) { e.scrollTop = 0; }",
                Vec::new(),
            )
            .await?;
        driver.wait_count(&checked("TMDB", "1"), 1).await?;
        driver.wait_count(&checked("TheTVDB", "1"), 1).await?;
        driver.wait_count(&checked("Scene", "1"), 1).await?;

        // Scrolled into TMDB S10's heading, which sits beside TheTVDB S10.
        driver
            .webdriver()
            .execute(
                "const s = document.querySelector('.modal .numbering-map-scroll'); \
                 const e = document.querySelector(\".modal .numbering-ep[aria-label='TMDB S10 E1']\"); \
                 s.scrollTop += e.getBoundingClientRect().top - s.getBoundingClientRect().top - 13;",
                Vec::new(),
            )
            .await?;
        driver.wait_count(&checked("TMDB", "10"), 1).await?;
        driver.wait_count(&checked("TheTVDB", "10"), 1).await?;
        driver.wait_count(&checked("Scene", ""), 1).await?;

        // Picking TMDB S20 scrolls the map to it.
        driver
            .find_one_by(".modal select[aria-label='TMDB season in view'] option[value='20']")
            .await?
            .click()
            .await?;
        driver.wait_count(&checked("TheTVDB", "20"), 1).await?;
        driver.wait_count(&checked("TMDB", "20"), 1).await?;
        ensure!(
            in_view(
                driver,
                ".modal .numbering-ep[aria-label='TMDB S20 E1']",
                Some(".numbering-map-scroll")
            )
            .await?,
            "the map did not scroll to TMDB S20 at {width}px"
        );

        let ret = driver
            .webdriver()
            .execute(
                "const e = document.querySelector('.modal .numbering-map-head'); return [e.scrollWidth, e.clientWidth];",
                Vec::new(),
            )
            .await?;
        let [scroll, client] = ret.convert::<[f64; 2]>()?;
        ensure!(
            scroll <= client,
            "the map's head is {scroll}px wide in {client}px"
        );

        driver
            .snapshot(&format!("numbering-season-in-view-{width}"))
            .await?;
    }

    Ok(())
}

/// A suggestion leaves the still-airing season open, drawn fading past its
/// last episode. Only the last range of a TMDB season can be open, and an
/// open range saved without a last episode maps episodes added after it.
pub async fn numbering_ranges_are_left_open(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    open_show(driver).await?;
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal .numbering-warning .link-button")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal [title='Manual ranges']")
        .await?
        .click()
        .await?;
    suggest_from(driver, "TheTVDB").await?;
    wait_spans(driver, &["S1 E1–E3", "S1 E1–E3", "S1 E4–", "S2 E1–"]).await?;
    // The open end fades over the episode after the last in each of the four
    // gaps, past what TMDB and XEM know.
    driver
        .wait_count(".modal .numbering-band.open-end", 4)
        .await?;
    let tail = driver
        .find_one_by(".modal .numbering-ep.open-end[aria-label='TMDB S1 E6']")
        .await?;
    ensure!(tail.text().await? == "E6…");
    ensure!(
        tail.attr("title").await? == "TMDB S1 E6 on: mapped by the open range as they are added"
    );
    driver
        .wait_count(
            ".modal .numbering-ep.open-end[aria-label='TheTVDB S2 E3']",
            1,
        )
        .await?;

    // Opening the first range would run it into the second.
    driver
        .find_one_by(".modal [title='Edit range 1']")
        .await?
        .click()
        .await?;
    let open_end =
        |n: u32| format!(".modal [title='Range {n} runs on through later episodes of the season']");
    driver.find_one_by(&open_end(1)).await?.click().await?;
    driver
        .wait_texts(
            ".modal .numbering-errors",
            ["E4 is already in the range S1 E1–. TMDB ranges must not overlap. An open range must be the last in its TMDB season."],
        )
        .await?;
    driver
        .wait_count(".modal [title='Save']:disabled", 1)
        .await?;
    driver.find_one_by(&open_end(1)).await?.click().await?;
    driver.wait_count(".modal .numbering-errors", 0).await?;
    wait_spans(driver, &["S1 E1–E3", "S1 E1–E3", "S1 E4–", "S2 E1–"]).await?;

    // Closing the second ends it at the last episode drawn, and opening it
    // again leaves the last episode out.
    driver
        .find_one_by(".modal [title='Edit range 2']")
        .await?
        .click()
        .await?;
    driver.find_one_by(&open_end(2)).await?.click().await?;
    wait_spans(driver, &["S1 E1–E3", "S1 E1–E3", "S1 E4–E5", "S2 E1–E2"]).await?;
    driver
        .wait_count(".modal .numbering-band.open-end", 0)
        .await?;
    driver.find_one_by(&open_end(2)).await?.click().await?;
    driver
        .wait_count(
            ".modal .numbering-range.selected [aria-label='Range 2 last episode']:disabled",
            1,
        )
        .await?;
    wait_spans(driver, &["S1 E1–E3", "S1 E1–E3", "S1 E4–", "S2 E1–"]).await?;

    driver
        .find_one_by(".modal [title='Save']")
        .await?
        .click()
        .await?;
    driver.find_one_by(".modal [title='Edit ranges']").await?;
    let saved = track.query("SELECT numbering FROM shows WHERE id = 1001")?;
    ensure!(
        saved
            == [
                r#"{"ranges":[{"season":1,"first":1,"last":3,"system":"tvdb","target_season":1,"target_first":1},{"season":1,"first":4,"system":"tvdb","target_season":2,"target_first":1}]}"#
            ],
        "saved {saved:?}"
    );

    // An episode TMDB adds later falls in the open range.
    track.execute(
        "INSERT INTO episodes (id, show_id, season, episode, aired) VALUES (3006, 1001, 1, 6, 1703024000000)",
    )?;
    driver.reload().await?;
    driver.find_one_by("#S01E06").await?;
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal [title='Edit ranges']")
        .await?
        .click()
        .await?;
    driver
        .wait_texts(
            ".modal .numbering-coverage",
            ["6 of 6 regular TMDB episodes are mapped."],
        )
        .await?;
    let added = driver
        .find_one_by(".modal .numbering-ep[aria-label='TMDB S1 E6']")
        .await?;
    ensure!(added.attr("title").await? == "TMDB S1 E6 ↔ TheTVDB S2 E3");
    driver
        .wait_count(".modal .numbering-ep.open-end[aria-label='TMDB S1 E7']", 1)
        .await
}

/// Forty one-episode ranges, each to the same TheTVDB episode, so the range
/// list runs well past the screen.
const LONG_NUMBERING: &str = "
    WITH RECURSIVE n(i) AS (SELECT 6 UNION ALL SELECT i + 1 FROM n WHERE i < 40)
    INSERT INTO episodes (id, show_id, season, episode, aired)
    SELECT 3000 + i, 1001, 1, i, 1700000000000 + i * 604800000 FROM n;
    DELETE FROM xem_episodes WHERE show_id = 1001 AND system = 'tvdb';
    WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 40)
    INSERT INTO xem_episodes (show_id, entry, system, part, season, episode, absolute)
    SELECT 1001, i - 1, 'tvdb', 0, 1, i, i FROM n;
    WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 40)
    UPDATE shows SET numbering = (
        SELECT json_object('ranges', json_group_array(json_object(
            'season', 1, 'first', i, 'last', i, 'system', 'tvdb',
            'target_season', 1, 'target_first', i)))
        FROM n
    ) WHERE id = 1001;";

/// Where the editor's map, its last range and the modal's visible content are,
/// as `[top, bottom, left, right]` each, and how far the content is scrolled.
async fn numbering_layout(driver: &TestDriver) -> Result<([[f64; 4]; 3], f64)> {
    let ret = driver
        .webdriver()
        .execute(
            "const r = s => { const b = document.querySelector(s).getBoundingClientRect(); \
                 return [b.top, b.bottom, b.left, b.right]; }; \
             const c = document.querySelector('.numbering-modal .modal-content'); \
             return [[r('.numbering-modal .numbering-map-pane'), \
                 r('.numbering-modal .numbering-range:last-child .numbering-range-head'), \
                 r('.numbering-modal .modal-content')], c.scrollTop];",
            Vec::new(),
        )
        .await?;
    Ok(ret.convert()?)
}

/// On a long show the map stays in view while the last ranges are edited:
/// beside the list on a wide screen, above it on a phone.
pub async fn numbering_map_stays_in_view(driver: &mut TestDriver, track: &mut Track) -> Result<()> {
    track.execute(LONG_NUMBERING)?;
    driver.set_window_size(1280, 900).await?;
    open_show(driver).await?;
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal [title='Edit ranges']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-range", 40).await?;

    let modal = driver.find_one_by(".numbering-modal").await?.rect().await?;
    ensure!(modal.width > 1000.0, "the editor is {}px wide", modal.width);

    driver
        .find_one_by(".modal [title='Edit range 40']")
        .await?
        .click()
        .await?;
    driver
        .wait_count(".modal .numbering-range.selected[aria-label='Range 40']", 1)
        .await?;
    let ([map, last, content], scrolled) = numbering_layout(driver).await?;
    ensure!(scrolled > 0.0, "the ranges fit without scrolling");
    ensure!(
        map[0] >= content[0] - 1.0 && map[1] <= content[1] + 1.0,
        "the map {map:?} left the modal {content:?}"
    );
    ensure!(
        map[3] <= last[2] && last[1] <= content[1] + 1.0,
        "the last range {last:?} is not in view beside the map {map:?}"
    );
    driver.snapshot("numbering-long-wide").await?;

    driver.set_window_size(400, 850).await?;
    driver
        .webdriver()
        .execute(
            "document.querySelector('.numbering-modal .numbering-range:last-child').scrollIntoView({ block: 'end' });",
            Vec::new(),
        )
        .await?;
    let ([map, last, content], scrolled) = numbering_layout(driver).await?;
    ensure!(scrolled > 0.0, "the ranges fit without scrolling");
    ensure!(
        map[0] >= content[0] - 1.0 && map[1] <= content[1] + 1.0,
        "the map {map:?} left the modal {content:?}"
    );
    ensure!(
        last[0] >= map[1] && last[1] <= content[1] + 1.0,
        "the last range {last:?} is not in view under the map {map:?}"
    );
    driver.snapshot("numbering-long-phone").await
}

/// XEM's other names for the show and its seasons are not on the page but in
/// the Other names modal: the show's with their language, then the season's
/// under the TheTVDB season they name.
pub async fn alternative_names_are_listed(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.find_one_by("#S01E05").await?;
    ensure_names_not_inline(driver).await?;

    open_names(driver).await?;
    driver
        .wait_texts(".names-modal h3", ["Show", "Season 1"])
        .await?;
    driver
        .wait_texts(
            ".names-modal .alt-name-text",
            [
                "Die Testserie",
                "La Série",
                "Shīdo Shō",
                "Seeded",
                "Seeded First",
            ],
        )
        .await?;
    driver
        .wait_texts(
            ".names-modal .translation-language",
            ["German", "French", "Japanese", "English", "Japanese"],
        )
        .await?;

    // Automatic numbering: TMDB Season 1 is TheTVDB Season 1.
    driver
        .wait_texts(".names-modal .names-target", ["TheTVDB Season 1"])
        .await?;
    driver.snapshot("show-names").await?;

    driver
        .find_one_by(".modal [title='Close']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal", 0).await
}

/// Each other name says where it comes from and its language, with the code its
/// source gave on hover; a missing code or one that names no language reads as
/// an unknown language.
pub async fn names_show_their_origin_and_language(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    track.execute(
        "INSERT INTO xem_names (show_id, season, language, name)
        VALUES (1001, NULL, NULL, 'Seeded Bare'), (1001, NULL, 'zz', 'Seeded Zed');",
    )?;

    open_show(driver).await?;
    driver.find_one_by("#S01E05").await?;
    open_names(driver).await?;

    let show = ".names-modal section:first-child";
    driver
        .wait_texts(
            format!("{show} .translation-language").as_str(),
            [
                "Unknown language",
                "German",
                "French",
                "Japanese",
                "English",
                "Unknown language",
            ],
        )
        .await?;
    driver
        .wait_texts(
            format!("{show} .alt-name-text").as_str(),
            [
                "Seeded Bare",
                "Die Testserie",
                "La Série",
                "Shīdo Shō",
                "Seeded",
                "Seeded Zed",
            ],
        )
        .await?;

    let titles = driver
        .find_all_attrs(&format!("{show} .translation-language"), "title")
        .await?;
    ensure!(
        titles
            == [
                "XEM gives no language",
                "XEM language code: de",
                "XEM language code: fr",
                "XEM language code: jp",
                "XEM language code: us",
                "XEM language code: zz",
            ],
        "the languages' titles are {titles:?}"
    );

    let unknown = driver
        .find_all_attrs(".names-modal .translation-language.unknown", "title")
        .await?;
    ensure!(
        unknown == ["XEM gives no language", "XEM language code: zz"],
        "the unknown languages are {unknown:?}"
    );

    let origins = driver
        .find_all_attrs(".names-modal .alt-name-origin", "title")
        .await?;
    ensure!(
        origins.len() == driver.count(".names-modal .alt-name").await?
            && origins.iter().all(|o| o == "From XEM"),
        "not every name says it is from XEM: {origins:?}"
    );

    driver.snapshot("show-names-origin").await
}

/// The Other names modal says what the names are, and its help mark opens the
/// section explaining them.
pub async fn names_modal_opens_its_help(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;
    driver.find_one_by("#S01E05").await?;
    open_names(driver).await?;

    driver
        .wait_texts(
            ".names-modal .names-intro",
            ["Other titles this show and its seasons go by, with each one's language and where it comes from."],
        )
        .await?;

    let mark = driver
        .find_one_by(".names-modal .names-intro button.help-mark")
        .await?;
    ensure!(
        mark.attr("title").await? == "Help: Other names",
        "the names modal's help mark does not name its section"
    );
    mark.click().await?;

    driver
        .wait_texts("[data-test=help-title]", ["Other names"])
        .await?;
    driver.snapshot("show-names-help").await
}

/// On phones the Other names modal opens from the show's actions menu; a show
/// without XEM names has no such entry.
pub async fn phones_open_names_from_actions(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    open_show(driver).await?;
    driver.set_window_size(400, 850).await?;
    driver.find_one_by("#S01E05").await?;
    ensure_names_not_inline(driver).await?;

    driver
        .find_one_by("button[title='Actions']")
        .await?
        .click()
        .await?;
    open_names(driver).await?;
    driver
        .wait_texts(".names-modal h3", ["Show", "Season 1"])
        .await?;
    driver.snapshot("phone-show-names").await?;

    track.execute("DELETE FROM xem_names;")?;
    driver.reload().await?;
    driver.wait_texts(".detail-title", ["Seeded Show"]).await?;
    driver.find_one_by("#S01E05").await?;
    driver
        .find_one_by("button[title='Actions']")
        .await?
        .click()
        .await?;
    driver
        .find_one_by("#page > .toolbar .toolbar-dropdown [title='Translations']")
        .await?;
    ensure!(driver.count("[title='Other names']").await? == 0);
    Ok(())
}

/// Opens the show's Other names modal.
async fn open_names(driver: &TestDriver) -> Result<()> {
    driver
        .find_one_by("[title='Other names']")
        .await?
        .click()
        .await?;
    driver.find_one_by(".modal.names-modal").await?;
    Ok(())
}

/// None of the seeded XEM names show on the page outside a modal.
async fn ensure_names_not_inline(driver: &TestDriver) -> Result<()> {
    let text = driver
        .webdriver()
        .execute(
            "return document.querySelector('#page').innerText;",
            Vec::new(),
        )
        .await?
        .convert::<String>()?;

    for name in [
        "Die Testserie",
        "Shīdo Shō",
        "Seeded First",
        "Seeded Second",
    ] {
        ensure!(!text.contains(name), "{name} shows on the page");
    }

    Ok(())
}

/// An XEM remote whose XEM show id is known links to its page, both in the
/// show's sources row and in the Remotes editor, which shows the id.
pub async fn xem_remote_links_to_its_page(driver: &mut TestDriver, _: &mut Track) -> Result<()> {
    open_show(driver).await?;

    let page = "a[href='https://thexem.info/xem/show/6743'] .logo.xem";
    driver
        .wait_count(&format!(".detail-sources {page}"), 1)
        .await?;

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

    driver.wait_count(&format!(".modal {page}"), 1).await?;
    driver
        .wait_texts(".modal [data-testid='remote-id']", ["tvdb/424536 #6743"])
        .await
}

/// Linked to TheTVDB with ranges, an episode XEM numbers differently shows
/// those numbers as chips, one that lines up shows none, and a line marks
/// where TheTVDB's second season starts.
pub async fn episodes_show_other_numberings(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    open_show(driver).await?;

    // Automatic numbering puts S01E04 at TheTVDB S1E4, which XEM doesn't map.
    driver.find_one_by("#S01E04").await?;
    ensure!(driver.count(".numbering-chip").await? == 0);
    ensure!(driver.count(".numbering-divider").await? == 0);

    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal .numbering-warning .link-button")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal .numbering-status [title='Suggest from episode order']")
        .await?
        .click()
        .await?;
    suggest_from(driver, "TheTVDB").await?;
    driver
        .find_one_by(".modal [title='Save']")
        .await?
        .click()
        .await?;
    driver.find_one_by(".modal [title='Edit ranges']").await?;
    ensure!(
        track.query("SELECT CAST(numbering IS NOT NULL AS TEXT) FROM shows WHERE id = 1001")?
            == ["1"]
    );

    driver
        .find_one_by(".modal [title='Close']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal", 0).await?;

    driver
        .wait_texts("#S01E04 .numbering-chip", ["S02E01", "S02E01"])
        .await?;
    ensure!(
        driver
            .find_all_attrs("#S01E04 .numbering-chip", "title")
            .await?
            == ["TheTVDB S02E01, absolute 4", "Scene S02E01, absolute 4"],
        "AniDB's S1E4 is the episode's own code"
    );
    driver
        .wait_texts("#S01E05 .numbering-chip", ["S02E02+03", "S02E02"])
        .await?;
    ensure!(driver.count("#S01E01 .numbering-chip").await? == 0);

    driver
        .wait_texts(".numbering-divider-season", ["Season 1", "Season 2"])
        .await?;

    // The divider comes right before the first episode of TheTVDB's season 2.
    let before = driver
        .webdriver()
        .execute(
            "return document.querySelectorAll('.numbering-divider')[1].nextElementSibling.id;",
            Vec::new(),
        )
        .await?;
    ensure!(before.convert::<String>()? == "S01E04");
    ensure_names_not_inline(driver).await?;

    // The season's names follow the numbering to both TheTVDB seasons.
    open_names(driver).await?;
    driver
        .wait_texts(
            ".names-modal .names-target",
            ["TheTVDB Season 1", "TheTVDB Season 2"],
        )
        .await?;
    driver
        .wait_texts(
            ".names-modal section:last-child .alt-name-text",
            ["Seeded First", "Seeded Second"],
        )
        .await
}

/// Ranges that map consecutive seasons onto the same seasons of TheTVDB fold
/// into one row, which unfolds on a click or when the map selects one of its
/// ranges; the rows' numbers line up in columns.
pub async fn numbering_ranges_fold_and_line_up(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    // Ten more seasons of three episodes. TheTVDB's S2 has two, its E3 being
    // the second part of E2, so S2 is out of step.
    track.execute(
        "INSERT INTO episodes (id, show_id, season, episode, aired)
         WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 29)
         SELECT 4000 + i, 1001, 2 + i / 3, 1 + i % 3, 1710000000000 + i * 604800000 FROM n;
         INSERT INTO xem_episodes (show_id, entry, system, part, season, episode, absolute)
         WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 26)
         SELECT 1001, 100 + i, 'tvdb', 0, 3 + i / 3, 1 + i % 3, 100 + i FROM n;",
    )?;

    let range = |season: u32, first: u32, last: Option<u32>, target: u32| {
        let last = last.map_or("null".to_owned(), |l| l.to_string());
        format!(
            r#"{{"season":{season},"first":{first},"last":{last},"system":"tvdb","target_season":{target},"target_first":1}}"#
        )
    };

    let ranges = [range(1, 1, Some(3), 1)]
        .into_iter()
        .chain((2..=9).map(|s| range(s, 1, Some(3), s)))
        .chain([range(10, 1, None, 10), range(11, 2, Some(3), 11)])
        .collect::<Vec<_>>()
        .join(",");
    track.execute(&format!(
        r#"UPDATE shows SET numbering = '{{"ranges":[{ranges}]}}' WHERE id = 1001"#
    ))?;

    driver.set_window_size(1280, 900).await?;
    open_show(driver).await?;
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal [title='Edit ranges']")
        .await?
        .click()
        .await?;

    let fold = ".modal .numbering-fold-summary";
    driver
        .wait_texts(fold, ["S3–S10 match TheTVDB season for season · 8 ranges"])
        .await?;
    ensure!(driver.count(".modal .numbering-range").await? == 3);
    let s1 = ["S1 E1–E3", "S1 E1–E3", "S2 E1–E3", "S2 E1–E3"];
    let s11 = ["S11 E2–E3", "S11 E1–E2"];
    wait_spans(driver, &[&s1[..], &s11[..]].concat()).await?;

    // The starts and ends of both sides line up from row to row.
    let ret = driver
        .webdriver()
        .execute(
            "return [...document.querySelectorAll('.modal .numbering-range-summary')].map(s => \
                 [...s.querySelectorAll('.numbering-first, .numbering-last')].map(e => { \
                     const b = e.getBoundingClientRect(); \
                     return e.classList.contains('numbering-first') ? b.right : b.left; }));",
            Vec::new(),
        )
        .await?;
    let edges = ret.convert::<Vec<[f64; 4]>>()?;
    ensure!(
        edges.windows(2).all(|w| w[0] == w[1]),
        "the ranges' numbers do not line up: {edges:?}"
    );

    driver.find_one_by(fold).await?.click().await?;
    driver.wait_count(".modal .numbering-range", 11).await?;
    let run = (3..=10)
        .flat_map(|s| {
            let span = if s == 10 { "E1–" } else { "E1–E3" };
            [format!("S{s} {span}"), format!("S{s} {span}")]
        })
        .collect::<Vec<_>>();
    let run = run.iter().map(String::as_str).collect::<Vec<_>>();
    wait_spans(driver, &[&s1[..], &run, &s11[..]].concat()).await?;
    driver.snapshot("numbering-fold-open-wide").await?;
    driver.find_one_by(fold).await?.click().await?;
    driver.wait_count(".modal .numbering-range", 3).await?;
    driver.snapshot("numbering-fold-wide").await?;

    // The map selects a range in the folded run: the run unfolds to it, and
    // folds again when done.
    pick(driver, "TMDB S5 E2").await?;
    let selected = ".modal .numbering-range.selected[aria-label='Range 5']";
    driver.wait_count(selected, 1).await?;
    ensure!(
        in_view(driver, selected, None).await?,
        "the list did not scroll to range 5"
    );

    // Editing it out of step splits the run around it.
    driver
        .find_one_by(".modal [aria-label='Range 5 last episode']")
        .await?
        .send_keys("\u{E003}2")
        .await?;
    driver
        .wait_texts(
            fold,
            [
                "S3–S4 match TheTVDB season for season · 2 ranges",
                "S6–S10 match TheTVDB season for season · 5 ranges",
            ],
        )
        .await?;
    driver.wait_count(selected, 1).await?;
    driver
        .find_one_by(".modal [title='Done editing range 5']")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-range", 4).await?;
    wait_spans(
        driver,
        &[&s1[..], &["S5 E1–E2", "S5 E1–E2"], &s11[..]].concat(),
    )
    .await?;

    // A phone fits the list without scrolling sideways.
    driver.set_window_size(400, 850).await?;
    driver.find_all(By::Css(fold)).await?[1].click().await?;
    driver.wait_count(".modal .numbering-range", 9).await?;
    let ret = driver
        .webdriver()
        .execute(
            "const e = document.querySelector('.numbering-modal .modal-content'); return [e.scrollWidth, e.clientWidth];",
            Vec::new(),
        )
        .await?;
    let [scroll, client] = ret.convert::<[f64; 2]>()?;
    ensure!(
        scroll <= client,
        "the list is {scroll}px wide in {client}px"
    );
    driver.snapshot("numbering-fold-phone").await?;
    Ok(())
}

/// The top in the window of the element `selector` matches, after scrolling
/// it to the middle of the map when `center`.
async fn map_row_top(driver: &TestDriver, selector: &str, center: bool) -> Result<f64> {
    let ret = driver
        .webdriver()
        .execute(
            "const e = document.querySelector(arguments[0]); \
             if (arguments[1]) e.scrollIntoView({ block: 'center' }); \
             return e.getBoundingClientRect().top;",
            vec![selector.into(), center.into()],
        )
        .await?;
    Ok(ret.convert()?)
}

/// How far below the top of the map's view the element `selector` matches is.
async fn map_offset(driver: &TestDriver, selector: &str) -> Result<f64> {
    let ret = driver
        .webdriver()
        .execute(
            "const s = document.querySelector('.modal .numbering-map-scroll'); \
             return document.querySelector(arguments[0]).getBoundingClientRect().top - s.getBoundingClientRect().top;",
            vec![selector.into()],
        )
        .await?;
    Ok(ret.convert()?)
}

/// Click the element `selector` matches without scrolling to it.
async fn js_click(driver: &TestDriver, selector: &str) -> Result<()> {
    driver
        .webdriver()
        .execute(
            "document.querySelector(arguments[0]).click();",
            vec![selector.into()],
        )
        .await?;
    Ok(())
}

/// Long runs of alike episodes fold in the map, keeping their first and last
/// to click, and bands run on through them. A fold unfolds on a click, with
/// the same ranges' folds in the other columns, without moving under the
/// pointer and keeping a span being picked; the map folds and unfolds all
/// at once, and six range colours keep neighbours apart.
pub async fn numbering_map_folds_long_runs(
    driver: &mut TestDriver,
    track: &mut Track,
) -> Result<()> {
    // Fifty episodes in TMDB's and TheTVDB's S1: E1-E30 in one range, E31-E35
    // one range each, E36-E50 unmapped.
    track.execute(
        "WITH RECURSIVE n(i) AS (SELECT 6 UNION ALL SELECT i + 1 FROM n WHERE i < 50)
         INSERT INTO episodes (id, show_id, season, episode, aired)
         SELECT 5000 + i, 1001, 1, i, 1700000000000 + i * 604800000 FROM n;
         DELETE FROM xem_episodes WHERE show_id = 1001 AND system = 'tvdb';
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 50)
         INSERT INTO xem_episodes (show_id, entry, system, part, season, episode, absolute)
         SELECT 1001, i - 1, 'tvdb', 0, 1, i, i FROM n;",
    )?;

    let range = |first: u32, last: u32| {
        format!(
            r#"{{"season":1,"first":{first},"last":{last},"system":"tvdb","target_season":1,"target_first":{first}}}"#
        )
    };
    let ranges = [range(1, 30)]
        .into_iter()
        .chain((31..=35).map(|e| range(e, e)))
        .collect::<Vec<_>>()
        .join(",");
    track.execute(&format!(
        r#"UPDATE shows SET numbering = '{{"ranges":[{ranges}]}}' WHERE id = 1001"#
    ))?;

    driver.set_window_size(1280, 900).await?;
    open_show(driver).await?;
    driver
        .find_one_by("[title='Settings']:not(#toolbar *)")
        .await?
        .click()
        .await?;
    driver
        .find_one_by(".modal [title='Edit ranges']")
        .await?
        .click()
        .await?;

    let folds =
        |side: &str| format!(".modal .numbering-column[data-side='{side}'] .numbering-fold-row");
    driver.wait_count(&folds("tmdb"), 2).await?;
    ensure!(
        driver.find_all_attrs(&folds("tmdb"), "aria-label").await?
            == ["TMDB S1 E2–E29", "TMDB S1 E37–E49"]
    );
    ensure!(
        driver.find_all_attrs(&folds("tvdb"), "aria-label").await?
            == ["TheTVDB S1 E2–E29", "TheTVDB S1 E37–E49"]
    );
    ensure!(
        driver
            .count(".modal .numbering-ep[aria-label='TMDB S1 E15']")
            .await?
            == 0
    );

    // The runs' ends stay, and the band runs on through the folds.
    for label in ["TMDB S1 E1", "TMDB S1 E30", "TMDB S1 E36", "TMDB S1 E50"] {
        ensure!(
            driver
                .count(&format!(".modal .numbering-ep[aria-label='{label}']"))
                .await?
                == 1,
            "{label} is folded"
        );
    }
    let ret = driver
        .webdriver()
        .execute(
            "return [...document.querySelectorAll('.modal .numbering-band title')].map(e => e.textContent);",
            Vec::new(),
        )
        .await?;
    let titles = ret.convert::<Vec<String>>()?;
    ensure!(
        titles
            .iter()
            .any(|t| t == "TMDB S1 E1–E30 ↔ TheTVDB S1 E1–E30"),
        "no band through the folds: {titles:?}"
    );

    // Six colours: the sixth range has its own.
    ensure!(driver.count(".modal .numbering-swatch.tone-5").await? == 1);
    ensure!(
        driver
            .count(".modal .numbering-ep.tone-5[aria-label='TMDB S1 E35']")
            .await?
            == 1
    );
    driver.snapshot("numbering-map-folded").await?;

    // Unfolding keeps the row in place and unfolds TheTVDB's too.
    let fold = ".modal .numbering-fold-row[aria-label='TMDB S1 E2–E29']";
    let before = map_row_top(driver, fold, true).await?;
    driver.find_one_by(fold).await?.click().await?;
    driver
        .wait_count(".modal .numbering-ep[aria-label='TMDB S1 E15']", 1)
        .await?;
    let after = map_row_top(
        driver,
        ".modal .numbering-ep[aria-label='TMDB S1 E2']",
        false,
    )
    .await?;
    ensure!(
        (after - before).abs() <= 1.0,
        "the unfolded row moved from {before} to {after}"
    );
    ensure!(
        driver
            .count(".modal .numbering-ep[aria-label='TheTVDB S1 E15']")
            .await?
            == 1
    );
    ensure!(driver.count(&folds("tmdb")).await? == 1);

    // Folding all again.
    driver
        .find_one_by(".modal .numbering-fold-all")
        .await?
        .click()
        .await?;
    driver.wait_count(&folds("tmdb"), 2).await?;
    ensure!(driver.count(".modal .numbering-fold-all").await? == 0);

    // Picking a span whose end is folded: the fold opens and keeps the first
    // click.
    pick(driver, "TMDB S1 E1").await?;
    driver
        .wait_count(".modal .numbering-range.selected[aria-label='Range 1']", 1)
        .await?;
    pick(driver, "TMDB S1 E1").await?;
    driver
        .wait_count(".modal .numbering-ep.anchor[aria-label='TMDB S1 E1']", 1)
        .await?;
    let fold = ".modal .numbering-fold-row[aria-label='TMDB S1 E3–E29']";
    driver.wait_count(fold, 1).await?;
    let before = map_row_top(driver, fold, true).await?;
    driver.find_one_by(fold).await?.click().await?;
    driver
        .wait_count(".modal .numbering-ep[aria-label='TMDB S1 E20']", 1)
        .await?;
    let after = map_row_top(
        driver,
        ".modal .numbering-ep[aria-label='TMDB S1 E3']",
        false,
    )
    .await?;
    ensure!(
        (after - before).abs() <= 1.0,
        "the unfolded row moved from {before} to {after}"
    );
    ensure!(
        driver
            .count(".modal .numbering-ep.anchor[aria-label='TMDB S1 E1']")
            .await?
            == 1
    );
    pick(driver, "TMDB S1 E20").await?;
    let singles = (31..=35)
        .flat_map(|e| [format!("S1 E{e}–E{e}"), format!("S1 E{e}–E{e}")])
        .collect::<Vec<_>>();
    let singles = singles.iter().map(String::as_str).collect::<Vec<_>>();
    wait_spans(
        driver,
        &[&["S1 E1–E20", "S1 E1–E20"][..], &singles].concat(),
    )
    .await?;

    // Unfolding everything.
    driver
        .find_one_by(".modal .numbering-unfold-all")
        .await?
        .click()
        .await?;
    driver.wait_count(".modal .numbering-fold-row", 0).await?;
    ensure!(
        driver
            .count(".modal .numbering-column[data-side='tmdb'] .numbering-ep")
            .await?
            == 50
    );
    driver.snapshot("numbering-map-unfolded").await?;

    // On a short phone, folding the runs above the map's top row leaves that
    // row where it was.
    driver.set_window_size(400, 500).await?;
    let row = ".modal .numbering-ep[aria-label='TMDB S1 E30']";
    let top = map_offset(driver, row).await?;
    driver
        .webdriver()
        .execute(
            "document.querySelector('.modal .numbering-map-scroll').scrollTop += arguments[0];",
            vec![top.into()],
        )
        .await?;
    let before = map_offset(driver, row).await?;
    ensure!(
        before.abs() <= 1.0,
        "the map did not scroll to E30: {before}"
    );
    js_click(driver, ".modal .numbering-fold-all").await?;
    driver.wait_count(&folds("tmdb"), 3).await?;
    let after = map_offset(driver, row).await?;
    ensure!(
        (after - before).abs() <= 1.0,
        "the map's top row moved from {before} to {after}"
    );
    driver.snapshot("numbering-map-folded-phone").await
}
