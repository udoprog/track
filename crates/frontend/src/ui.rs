use web_sys::MouseEvent;
use yew::prelude::*;

use iso639::{LanguageToCountry, Languages};

#[derive(Properties, PartialEq)]
pub(super) struct PaginationButtonsProps {
    pub(super) page: usize,
    pub(super) total_pages: usize,
    pub(super) on_page: Callback<usize>,
}

#[function_component]
pub(super) fn PaginationButtons(props: &PaginationButtonsProps) -> Html {
    let page = props.page.min(props.total_pages.saturating_sub(1));
    let prev = page.checked_sub(1);
    let next = (page + 1 < props.total_pages).then_some(page + 1);
    let on_page = props.on_page.clone();
    let on_page2 = props.on_page.clone();

    if props.total_pages <= 1 {
        return html! {};
    }

    html! {
        <>
            <button class="btn-icon" disabled={prev.is_none()}
                onclick={Callback::from(move |_| { if let Some(p) = prev { on_page.emit(p); } })}>
                <span class="icon arrow-left" />
            </button>
            <span class="text-muted">{format!("{} / {}", page + 1, props.total_pages)}</span>
            <button class="btn-icon" disabled={next.is_none()}
                onclick={Callback::from(move |_| { if let Some(p) = next { on_page2.emit(p); } })}>
                <span class="icon arrow-right" />
            </button>
        </>
    }
}

#[derive(Properties, PartialEq)]
pub(super) struct ConfirmDangerProps {
    pub(super) label: Option<AttrValue>,
    pub(super) on_confirm: Callback<()>,
    pub(super) on_cancel: Callback<()>,
    pub(super) prompt: AttrValue,
    #[prop_or_default]
    pub(super) btn_class: Classes,
}

#[function_component]
pub(super) fn ConfirmDanger(props: &ConfirmDangerProps) -> Html {
    let on_confirm = {
        let cb = props.on_confirm.clone();

        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    let on_cancel = {
        let cb = props.on_cancel.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    html! {
        <div class="row-fill fill">
            if let Some(ref label) = props.label {
                <span class="fill">{&props.prompt}{" "}{label}{"?"}</span>
            } else {
                <span class="fill">{&props.prompt}{"?"}</span>
            }

            <div class="input-group end">
                <button onclick={on_cancel} class={classes!("btn-icon", &props.btn_class)} title="No">
                    <span class="icon x-mark" />
                </button>

                <button onclick={on_confirm} class={classes!("btn-icon-danger", &props.btn_class)} title="Yes">
                    <span class="icon check" />
                </button>
            </div>
        </div>
    }
}

// ── MarkWatchedPicker ─────────────────────────────────────────────────────────

/// Two-button step shown after clicking "Mark watched": choose now or when aired.
/// Renders as a `row-fill fill` that can replace the watch button's action area.
#[derive(Properties, PartialEq)]
pub(super) struct MarkWatchedPickerProps {
    pub(super) aired: Option<api::Timestamp>,
    pub(super) on_confirm: Callback<Option<api::Timestamp>>,
    pub(super) on_cancel: Callback<()>,
}

#[function_component]
pub(super) fn MarkWatchedPicker(props: &MarkWatchedPickerProps) -> Html {
    let aired = props.aired;

    let on_now = {
        let cb = props.on_confirm.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(None);
        })
    };

    let on_aired = aired.map(|ts| {
        let cb = props.on_confirm.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(Some(ts));
        })
    });

    let on_cancel = {
        let cb = props.on_cancel.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            cb.emit(());
        })
    };

    html! {
        <div class="row-fill fill">
            <span class="fill">{"Watched when?"}</span>

            <div class="input-group end">
                <button class="btn-icon" onclick={on_cancel} title="Cancel">
                    <span class="icon x-mark" />
                </button>
                <button class="btn-success" onclick={on_now} title="Watched now">
                    <span class="icon-inline"><span class="icon check" /></span>
                    {"Now"}
                </button>
                if let Some(on_a) = on_aired {
                    <button class="btn" onclick={on_a} title="Watched when aired">
                        <span class="icon-inline"><span class="icon clock" /></span>
                        {"Aired"}
                    </button>
                }
            </div>
        </div>
    }
}

// ── RemoteSourceSelect ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteSourceKind {
    Series,
    Movie,
}

#[derive(Properties, PartialEq)]
pub(super) struct RemoteSourceSelectProps {
    pub(super) remotes: Vec<api::RemoteId>,
    pub(super) current_source: Option<api::SyncSource>,
    pub(super) kind: RemoteSourceKind,
    pub(super) on_change: Callback<api::SyncSource>,
}

#[function_component]
pub(super) fn RemoteSourceSelect(props: &RemoteSourceSelectProps) -> Html {
    let mut options: Vec<api::SyncSource> = Vec::new();

    for remote in &props.remotes {
        let Some(source) = api::SyncSource::from_remote_source(remote.source()) else {
            continue;
        };

        if source == api::SyncSource::Tmdb
            || (matches!(props.kind, RemoteSourceKind::Series) && source == api::SyncSource::Tvdb)
        {
            if !options.iter().any(|existing| existing == &source) {
                options.push(source);
            }
        }
    }

    if options.is_empty() {
        return Html::default();
    }

    let selected = props
        .current_source
        .as_ref()
        .filter(|source| options.iter().any(|option| option == *source))
        .copied()
        .unwrap_or(options[0]);

    let disabled = options.len() <= 1;

    let on_change = {
        let cb = props.on_change.clone();

        Callback::from(move |e: Event| {
            let input: web_sys::HtmlSelectElement = e.target_unchecked_into();
            if let Some(source) = api::SyncSource::from_str(&input.value()) {
                cb.emit(source);
            }
        })
    };

    html! {
        <select class="input-select" onchange={on_change} {disabled} title="Select remote source">
            {
                for options.into_iter().map(|source| {
                    let label = source.as_str().to_uppercase();

                    html! {
                        <option value={source.as_str()} selected={source == selected}>{label}</option>
                    }
                })
            }
        </select>
    }
}

// ── ImageGallery ──────────────────────────────────────────────────────────────

/// A single entry shown in the image gallery.
#[derive(Clone, PartialEq)]
pub(super) struct ImageItem {
    pub(super) id: api::ImageId,
    pub(super) kind: api::ImageKind,
    pub(super) source: api::ImageSource,
    pub(super) image: api::Image,
    pub(super) selected: bool,
}

#[derive(Properties, PartialEq)]
pub(super) struct ImageGalleryProps {
    pub(super) items: Vec<ImageItem>,
    pub(super) kind: api::ImageKind,
    pub(super) on_select: Callback<api::ImageId>,
    pub(super) on_clear: Option<Callback<()>>,
    pub(super) on_close: Callback<()>,
}

#[function_component]
pub(super) fn ImageGallery(props: &ImageGalleryProps) -> Html {
    let imgs: Vec<_> = props
        .items
        .iter()
        .filter(|img| img.kind == props.kind)
        .collect();
    let on_select = props.on_select.clone();
    let on_close = props.on_close.clone();

    html! {
        <div class="modal-background" onclick={Callback::from(move |_| on_close.emit(()))}>
            <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <div class="row">
                    <span class="fill">{format!("Select {}", props.kind)}</span>

                    if let Some(on_clear) = props.on_clear.clone() {
                        <button class="btn" onclick={Callback::from(move |_| on_clear.emit(()))}>
                            <span class="icon-inline"><span class="icon x-mark" /></span>
                            {"Clear"}
                        </button>
                    }

                    <button class="btn-icon" onclick={props.on_close.reform(|_| ())}>
                        <span class="icon x-mark" />
                    </button>
                </div>

                if imgs.is_empty() {
                    <div class="empty text-muted">{"No images available."}</div>
                } else {
                    <div class="image-gallery">
                        { for imgs.iter().map(|img| {
                            let id = img.id;
                            let selected = img.selected;
                            let on_select = on_select.clone();
                            let title = img.source.to_string();
                            html! {
                                <div
                                    class={classes!("image-thumb", selected.then_some("image-thumb-selected"))}
                                    onclick={Callback::from(move |_| on_select.emit(id))}
                                    {title}
                                >
                                    <img src={img.image.proxy_url()} alt="" />
                                    if selected {
                                        <span class="image-thumb-check">{"✓"}</span>
                                    }
                                </div>
                            }
                        })}
                    </div>
                }
            </div>
        </div>
    }
}

// ── LanguagePicker ────────────────────────────────────────────────────────────

const LANGUAGE_PAGE_SIZE: usize = 5;

#[derive(Properties, PartialEq)]
pub(super) struct LanguagePickerProps {
    pub(super) current: Option<String>,
    pub(super) on_change: Callback<Option<String>>,
    pub(super) placeholder: &'static str,
}

pub(super) enum Msg {
    Open,
    Close,
    Filter(String),
    Page(usize),
    Pick(Option<String>),
}

pub(super) struct LanguagePicker {
    languages: Languages,
    language_to_country: LanguageToCountry,
    open: bool,
    filter: String,
    page: usize,
}

impl Component for LanguagePicker {
    type Message = Msg;
    type Properties = LanguagePickerProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            languages: Languages::new(),
            language_to_country: LanguageToCountry::new(),
            open: false,
            filter: String::new(),
            page: 0,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Open => {
                self.open = true;
                self.filter.clear();
                self.page = 0;
            }
            Msg::Close => {
                self.open = false;
            }
            Msg::Filter(s) => {
                self.filter = s;
                self.page = 0;
            }
            Msg::Page(p) => {
                self.page = p;
            }
            Msg::Pick(value) => {
                self.open = false;
                ctx.props().on_change.emit(value);
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let props = ctx.props();

        let value = ctx.props().current.as_ref().and_then(|code| {
            self.languages
                .get_by_part1(code)
                .and_then(|entry| Some((entry.ref_name, entry.part1?)))
        });

        let trigger = match value {
            Some((label, code)) => html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="icon-inline"><span class="icon language" /></span>
                    <span class="hide-mobile">{label}</span>


                    if let Some(code) = self.language_to_country.get_by_part1(code) {
                        <span class={classes!("flag", code)}></span>
                    }
                </button>
            },
            None => html! {
                <button class="btn" onclick={link.callback(|_| Msg::Open)} title="Select language">
                    <span class="icon-inline"><span class="icon language" /></span>
                    <span class="hide-mobile">{props.placeholder}</span>
                </button>
            },
        };

        if !self.open {
            return trigger;
        }

        let needle = self.filter.to_lowercase();
        let filtered: Vec<(&'static str, &'static iso639::Entry)> = self
            .languages
            .iter()
            .filter(|(_, entry)| {
                needle.is_empty() || entry.ref_name.to_lowercase().contains(&needle)
            })
            .collect();

        let total_pages = filtered.len().div_ceil(LANGUAGE_PAGE_SIZE).max(1);
        let page = self.page.min(total_pages.saturating_sub(1));

        let on_filter = link.callback(|e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::Filter(input.value())
        });

        let current = props.current.clone();

        html! {
            <>
                {trigger}
                <div class="modal-background" onclick={link.callback(|_| Msg::Close)}>
                    <div class="modal" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                        <div class="row">
                            <span class="fill">{"Language"}</span>

                            <button class="btn-icon" onclick={link.callback(|_| Msg::Close)}>
                                <span class="icon x-mark" />
                            </button>
                        </div>

                        <div class="row">
                            <input type="text" class="input-text fill" placeholder="Filter" value={self.filter.clone()} oninput={on_filter} />
                        </div>

                        <div class="table">
                            <div class="table-entry row clickable" onclick={link.callback(|_| Msg::Pick(None))}>
                                <span class="fill">{props.placeholder}</span>

                                if current.is_none() {
                                    <span class="icon check" />
                                }
                            </div>

                            {
                                for filtered.iter()
                                    .skip(page.saturating_mul(LANGUAGE_PAGE_SIZE))
                                    .take(LANGUAGE_PAGE_SIZE)
                                    .map(|&(part1, entry)| {
                                        let selected = current.as_deref() == Some(part1);

                                        html! {
                                            <div key={part1} class={classes!("table-entry", "row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| Msg::Pick(Some(part1.to_string())))}>
                                                <span class="fill">{entry.ref_name}</span>

                                                if let Some(code) = self.language_to_country.get_by_part1(part1) {
                                                    <span class={classes!("flag-inline", "flag", code)} />
                                                }

                                                <span class="text-muted">{part1}</span>
                                            </div>
                                        }
                                    })
                            }
                        </div>

                        <div class="row center">
                            <PaginationButtons
                                page={page}
                                total_pages={total_pages}
                                on_page={link.callback(Msg::Page)}
                            />
                        </div>
                    </div>
                </div>
            </>
        }
    }
}
