use wasm_bindgen_futures::{JsFuture, spawn_local};
use yew::prelude::*;

use sockel::{Button, ButtonVariant, Cluster, CodeSnippet, Gap, SectionHead, Stack};

use crate::{GITHUB, class};

const INSTALL: &str = "curl -fsSL https://udoprog.github.io/track/install.sh | sh";

const FROM_SOURCE: &str = "git clone https://github.com/udoprog/track
cd track
trug build --release
cargo install --locked --path crates/track";

/// The strengths, as the crate docs give them: one frame each.
const STRENGTHS: &[(&str, &str)] = &[
    (
        "One binary",
        "The server, the web app and a SQLite database. Run it, open a browser, and you're in.",
    ),
    (
        "Your data stays yours",
        "No ads, no recommendations engine and no account with someone else's service. Everything lives in one database file, and backups are plain JSON lines.",
    ),
    (
        "The whole household",
        "Everyone gets their own library, queue and history, and an admin hands out the sign-in links.",
    ),
];

struct Feature {
    title: &'static str,
    text: &'static str,
    /// The manual page that explains it.
    page: Option<(&'static str, &'static str)>,
}

/// What it does: one ticket each.
const FEATURES: &[Feature] = &[
    Feature {
        title: "Know what's next",
        text: "What's Next puts the next episode of every show you watch up front, poster first. Upcoming and a weekly Schedule make sure a new season never sneaks past you.",
        page: Some(("watching", "Marking things watched")),
    },
    Feature {
        title: "An honest history",
        text: "Mark an episode or movie watched now, when it aired or at any time you pick, a whole season in one go. History lists every watch, newest first.",
        page: Some(("watching#pending", "What pending means")),
    },
    Feature {
        title: "Metadata that gets it right",
        text: "Synced from TheTVDB, TMDB and TVmaze, in the language you prefer. Attach several sources to a title and choose which one leads.",
        page: Some(("remotes", "Remotes and syncing")),
    },
    Feature {
        title: "Anime fans welcome",
        text: "Episode numbering from XEM, AniDB and scene releases, with an editor to map one numbering onto another.",
        page: Some(("numberings", "Other numberings")),
    },
    Feature {
        title: "Phones are first class",
        text: "Every page is made for a narrow screen too. Live updates mean that what you mark on the sofa is already marked on the desktop.",
        page: None,
    },
    Feature {
        title: "Easy to run",
        text: "Multiple users, registration links and optional Cloudflare Access sign-in, plus a kick profile that deploys it as a systemd service.",
        page: Some(("settings", "Settings")),
    },
];

/// The made-up titles on the poster wall, with the hue of each poster.
const POSTERS: &[(&str, &str, u16)] = &[
    ("The Lighthouse Keepers", "Season 2", 205),
    ("Paper Moons", "Movie", 330),
    ("Northbound", "Season 1", 160),
    ("Glass Orchard", "Season 4", 35),
    ("Small Hours", "Movie", 265),
    ("Kite Season", "Season 3", 10),
    ("Copper Coast", "Movie", 120),
    ("The Ninth Floor", "Season 1", 290),
];

#[function_component]
fn CopyButton(props: &CopyProps) -> Html {
    let label = use_state(|| "Copy");

    let onclick = {
        let label = label.clone();
        let text = props.text.clone();

        Callback::from(move |_: MouseEvent| {
            let Some(window) = web_sys::window() else {
                return;
            };

            let promise = window.navigator().clipboard().write_text(&text);
            let label = label.clone();

            spawn_local(async move {
                match JsFuture::from(promise).await {
                    Ok(_) => label.set("Copied"),
                    Err(_) => label.set("Press Ctrl+C"),
                }
            });
        })
    };

    html! {
        <Button {onclick}>{ *label }</Button>
    }
}

#[derive(Properties, PartialEq)]
struct CopyProps {
    text: AttrValue,
}

#[derive(Properties, PartialEq)]
struct PosterProps {
    title: AttrValue,
    detail: AttrValue,
    hue: u16,
    #[prop_or_default]
    class: Classes,
}

/// A made-up poster: a ground in `hue`, a sun and hills, and the title.
#[function_component]
fn Poster(props: &PosterProps) -> Html {
    html! {
        <div class={classes!(class::POSTER, props.class.clone())} style={format!("--poster-hue: {}", props.hue)}>
            <span class={class::POSTER_ART} />
            <span class={class::POSTER_TITLE}>{ props.title.clone() }</span>
            <span class={class::POSTER_DETAIL}>{ props.detail.clone() }</span>
        </div>
    }
}

/// The hero's television, showing a What's Next card.
fn television() -> Html {
    html! {
        <div class={class::TV} aria-hidden="true">
            <div class={class::TV_SCREEN}>
                <div class={class::UP_NEXT}>
                    <Poster title="The Lighthouse Keepers" detail="S02" hue={205} class={class::POSTER_SMALL} />
                    <Stack gap={Gap::Tight}>
                        <span class={class::ON_AIR}>{ "What's next" }</span>
                        <strong>{ "The Lighthouse Keepers" }</strong>
                        <span class={class::MUTED}>{ "S02E05 · Low Tide" }</span>
                        <span class={class::MUTED}>{ "Aired yesterday" }</span>
                        <span class={class::UP_NEXT_BUTTON}>{ "✓ Mark watched" }</span>
                    </Stack>
                </div>
            </div>
            <div class={class::TV_STAND} />
        </div>
    }
}

#[function_component]
pub fn Home() -> Html {
    html! {
        <main class={class::HOME}>
            <section class={class::HERO}>
                <Stack gap={Gap::Loose}>
                    <div class={class::MARQUEE}>
                        <span class={class::MARQUEE_LABEL}>{ "Now showing" }</span>
                        <h1 class={class::HERO_TITLE}>{ "Know what to watch tonight" }</h1>
                    </div>
                    <p class={class::LEAD}>
                        { "track is a self-hosted tracker for the shows and movies you follow. It \
                           keeps your watch history, tells you what's next and shows you what's \
                           airing this week, all from one small server you run yourself." }
                    </p>
                    <Cluster>
                        <Button variant={ButtonVariant::Primary} href="#install">{ "Get track" }</Button>
                        <Button href="#/manual/">{ "Read the manual" }</Button>
                        <Button variant={ButtonVariant::Ghost} href={GITHUB}>{ "GitHub" }</Button>
                    </Cluster>
                </Stack>
                { television() }
            </section>

            <section class={class::SECTION}>
                <ol class={class::STRIP}>
                    { for STRENGTHS.iter().map(|(title, text)| html! {
                        <li class={class::FRAME}>
                            <strong class={class::FRAME_TITLE}>{ *title }</strong>
                            <p class={class::MUTED}>{ *text }</p>
                        </li>
                    }) }
                </ol>
            </section>

            <section class={class::SECTION}>
                <Stack gap={Gap::Loose}>
                    <SectionHead title={html! { "Tonight's programme" }} />
                    <ul class={class::TICKETS}>
                        { for FEATURES.iter().enumerate().map(|(n, feature)| html! {
                            <li class={class::TICKET}>
                                <div class={class::TICKET_BODY}>
                                    <strong class={class::TICKET_TITLE}>{ feature.title }</strong>
                                    <p class={class::MUTED}>{ feature.text }</p>
                                    if let Some((id, label)) = feature.page {
                                        <a href={format!("#/manual/{id}")}>{ format!("{label} →") }</a>
                                    }
                                </div>
                                <div class={class::TICKET_STUB} aria-hidden="true">
                                    <span>{ "Admit one" }</span>
                                    <span class={class::TICKET_NUMBER}>{ format!("No. {:03}", n + 1) }</span>
                                </div>
                            </li>
                        }) }
                    </ul>
                </Stack>
            </section>

            <section class={class::SECTION}>
                <div class={class::SHELF}>
                    <Stack gap={Gap::Loose}>
                        <SectionHead title={html! { "Your library, on your shelf" }} />
                        <p>
                            { "Track only follows what you add. Search TMDB and TheTVDB, press track, \
                               and the show joins your library with its posters, seasons, cast and \
                               air dates, synced from the sources you choose. Pick the posters and \
                               backdrops you like from each title's gallery." }
                        </p>
                        <p>
                            <a href="#/manual/finding">{ "Finding and adding shows and movies →" }</a>
                        </p>
                    </Stack>
                    <div class={class::POSTER_WALL} aria-hidden="true">
                        { for POSTERS.iter().map(|(title, detail, hue)| html! {
                            <Poster title={*title} detail={*detail} hue={*hue} />
                        }) }
                    </div>
                </div>
            </section>

            <section class={class::SECTION} id="install">
                <Stack gap={Gap::Loose} flush=true>
                    <SectionHead title={html! { "Install" }} />
                    <p>
                        { "On Linux and macOS, install the latest release into " }
                        <code>{ "~/.local/bin" }</code>
                        { ":" }
                    </p>
                    <Cluster nowrap=true class={class::COMMAND}>
                        <CodeSnippet code={INSTALL} class={classes!(class::TERMINAL, sockel::class::GROW)} />
                        <CopyButton text={INSTALL} />
                    </Cluster>
                    <p class={class::MUTED}>
                        { "Set " }<code>{ "TRACK_INSTALL_DIR" }</code>{ " to install elsewhere, or " }
                        <code>{ "TRACK_VERSION" }</code>
                        { " to a release tag to pin one. On Windows, download the zip from the " }
                        <a href={format!("{GITHUB}/releases")}>{ "releases" }</a>
                        { "." }
                    </p>
                    <p>
                        { "Or build it from source. The server embeds the web app, so build that \
                           first with " }
                        <a href="https://github.com/udoprog/trug">{ "trug" }</a>
                        { ":" }
                    </p>
                    <Cluster nowrap=true class={class::COMMAND}>
                        <CodeSnippet code={FROM_SOURCE} class={classes!(class::TERMINAL, sockel::class::GROW)} />
                        <CopyButton text={FROM_SOURCE} />
                    </Cluster>
                    <p>
                        { "Run " }<code>{ "track" }</code>{ " and open " }
                        <code>{ "http://127.0.0.1:3000" }</code>
                        { ". On first start the server logs a registration link for the \
                           administrator; then add your TheTVDB and TMDB API keys under Settings \
                           and search for something you're watching." }
                    </p>
                    <p>
                        <a href="#/manual/settings">{ "Settings and API keys →" }</a>
                    </p>
                </Stack>
            </section>
        </main>
    }
}
