use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[path = "../manual.rs"]
mod manual;

use manual::{Samples, image, link, parse, render, slug};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

fn ids() -> BTreeSet<String> {
    ["finding", "settings"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn icon(name: &str) -> Option<String> {
    (name != "missing").then(|| format!("<svg data-icon=\"{name}\"></svg>"))
}

fn samples() -> Samples<'static> {
    Samples { icon: &icon }
}

#[test]
fn links() {
    let ids = ids();
    assert_eq!(link("settings", &ids).unwrap(), "#/manual/settings");
    assert_eq!(
        link("settings#theme", &ids).unwrap(),
        "#/manual/settings#theme"
    );
    assert_eq!(
        link("https://thexem.info", &ids).unwrap(),
        "https://thexem.info"
    );
    assert!(link("nowhere", &ids).is_err());
}

#[test]
fn images() {
    assert_eq!(image("search.webp"), "help/search.webp");
    assert_eq!(image("./images/search.webp"), "help/images/search.webp");
    assert_eq!(
        image("https://example.com/a.png"),
        "https://example.com/a.png"
    );
}

#[test]
fn slugs() {
    assert_eq!(slug("What comes from where"), "what-comes-from-where");
    assert_eq!(slug("Linking a show to `XEM`"), "linking-a-show-to-xem");
}

#[test]
fn header_is_read_like_help_rs() {
    let section = parse("id: a\ntitle: A page\n\nbody\n").unwrap();
    assert_eq!(section.id, "a");
    assert_eq!(section.title, "A page");
    assert_eq!(section.body, "body\n");
    assert!(parse("id: a\n\nbody").is_none());
    assert!(parse("no header").is_none());
}

#[test]
fn the_help_syntax_is_drawn() {
    let page = render(
        "## Tracking\n\nOpen ![Search](icon:magnifying-glass) and press ![Track](button:plus), see [Settings](settings). ![A picture](shot.webp)\n\n```track\ntracked\n```\n\n<b>raw</b>\n\n## Tracking\n",
        &ids(),
        &samples(),
    )
    .unwrap();

    let html = &page.html;
    assert!(html.contains("<h2 id=\"tracking\">"), "{html}");
    assert!(html.contains("<h2 id=\"tracking-1\">"), "{html}");
    assert!(
        html.contains("aria-label=\"Search\"><svg data-icon=\"magnifying-glass\">"),
        "{html}"
    );
    assert!(
        html.contains(
            "<span class=\"site-key\"><svg data-icon=\"plus\"></svg><span>Track</span></span>"
        ),
        "{html}"
    );
    assert!(html.contains("href=\"#/manual/settings\""), "{html}");
    assert!(html.contains("src=\"help/shot.webp\""), "{html}");
    assert!(html.contains("class=\"site-demo\""), "{html}");
    assert!(html.contains("&lt;b&gt;raw&lt;/b&gt;"), "{html}");
    assert_eq!(
        page.headings,
        [
            ("tracking".to_owned(), "Tracking".to_owned()),
            ("tracking-1".to_owned(), "Tracking".to_owned())
        ]
    );
}

#[test]
fn mistakes_are_reported() {
    let ids = ids();
    assert!(render("![X](icon:missing)", &ids, &samples()).is_err());
    assert!(render("[X](nowhere)", &ids, &samples()).is_err());
    assert!(render("```track\nnope\n```\n", &ids, &samples()).is_err());
}

/// The site renders every page the app lists, in the app's order, and each
/// one renders with the real icons.
#[test]
fn every_help_page_renders() {
    let help = Path::new(ROOT).join(manual::HELP);
    let listing = fs::read_to_string(Path::new(ROOT).join("crates/frontend/src/help.rs")).unwrap();

    let mut files = fs::read_dir(&help)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".md"))
        .collect::<Vec<_>>();

    files.sort();

    let listed = listing
        .lines()
        .map(|line| line.trim())
        .filter_map(|line| line.strip_prefix('(').unwrap_or(line).strip_prefix('"'))
        .filter_map(|line| line.split_once('"'))
        .map(|(name, _)| name.to_owned())
        .filter(|name| name.ends_with(".md"))
        .collect::<Vec<_>>();

    assert_eq!(
        files, listed,
        "help.rs lists the help files in another order"
    );

    let sections = files
        .iter()
        .map(|name| parse(&fs::read_to_string(help.join(name)).unwrap()).unwrap())
        .collect::<Vec<_>>();

    let ids = sections
        .iter()
        .map(|s| s.id.clone())
        .collect::<BTreeSet<_>>();

    let icons = Path::new(ROOT).join("crates/frontend/icons");
    let icon = |name: &str| fs::read_to_string(icons.join(format!("{name}.svg"))).ok();
    let samples = Samples { icon: &icon };

    for section in &sections {
        if let Err(error) = render(&section.body, &ids, &samples) {
            panic!("{}: {error}", section.id);
        }
    }
}
