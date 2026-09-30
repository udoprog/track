//! An overview-style paragraph resolved from a [`api::Translations`] set, with
//! a non-persistent language picker: when alternate translations are available
//! a subtle button floated at the top right of the text (so the text rows flow
//! around it instead of it reserving a row of its own), showing the flag of the
//! displayed language only when it is not the configured one, opens an anchored
//! language menu that switches the displayed text for this section only. The
//! choice lives in component state, so it resets on navigation and is never
//! persisted.

use yew::prelude::*;

use super::{Button, ContextMenu, locale_label};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The translations the text is resolved from.
    pub(crate) strings: api::Translations,
    /// The kind of string displayed.
    #[prop_or(api::StringKind::Overview)]
    pub(crate) kind: api::StringKind,
    /// Classes for the text paragraph.
    #[prop_or_else(|| classes!("overview"))]
    pub(crate) class: Classes,
}

pub(crate) enum Msg {
    Open,
    Close,
    Pick(Option<api::Locale>),
}

pub(crate) struct TranslatedText {
    /// The trigger button, anchored to by the popover.
    anchor: NodeRef,
    open: bool,
    /// The section-local language override. `None` follows the configured
    /// display locale.
    selected: Option<api::Locale>,
}

impl Component for TranslatedText {
    type Message = Msg;
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {
            anchor: NodeRef::default(),
            open: false,
            selected: None,
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Open => {
                self.open = true;
            }
            Msg::Close => {
                self.open = false;
            }
            Msg::Pick(locale) => {
                self.selected = locale;
                self.open = false;
            }
        }

        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old: &Props) -> bool {
        // The override is tied to the strings it was picked from; when they
        // change (navigating to another entity reusing this component, or a
        // sync refresh) reset to the default resolution.
        if ctx.props().strings != old.strings {
            self.selected = None;
            self.open = false;
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let link = ctx.link();

        let text = match self.selected {
            Some(locale) => props.strings.get_with(props.kind, locale),
            None => props.strings.get(props.kind),
        };

        let mut locales = props.strings.locales(props.kind).collect::<Vec<_>>();
        locales.sort();

        // Offer the picker when there is anything to switch to: several
        // languages, or a single one that the default resolution misses.
        let picker = locales.len() > 1 || (!locales.is_empty() && text.is_none());

        if !picker {
            return match text {
                Some(text) => html!(<p class={props.class.clone()}>{text}</p>),
                None => Html::default(),
            };
        }

        let displayed = self
            .selected
            .or_else(|| props.strings.resolved_locale(props.kind));

        // Text in the configured language needs no flag calling it out.
        let foreign =
            displayed.filter(|locale| locale.language() != props.strings.locale().language());

        let toggle = html! {
            <Button node_ref={self.anchor.clone()} class="language-toggle" title="Change displayed language" expanded={Some(self.open)} haspopup="dialog" onclick={link.callback(|_| Msg::Open)}>
                if let Some(locale) = foreign {
                    if let Some(flag) = locale.flag() {
                        <span class={classes!("flag", flag)} />
                    } else {
                        <span class="text-muted">{locale}</span>
                    }
                } else {
                    <span class="icon sm language" />
                }
            </Button>
        };

        html! {
            <>
                if let Some(text) = text {
                    <p class={props.class.clone()}>{toggle}{text}</p>
                } else {
                    <div class="text-muted">{toggle}{"No translation for this language."}</div>
                }

                if self.open {
                    <ContextMenu icon="language" prompt="Displayed language" anchor={self.anchor.clone()} on_close={link.callback(|_| Msg::Close)}>
                        <div class="table">
                            <div class={classes!("row", "clickable", self.selected.is_none().then_some("active"))} onclick={link.callback(|_| Msg::Pick(None))}>
                                <span class="fill">{"Default"}</span>

                                if self.selected.is_none() {
                                    <span class="item-inline">
                                        <span class="icon check" />
                                    </span>
                                }

                                <span class="item-inline">
                                    <span class="icon icon-4x3 language" />
                                </span>
                            </div>

                            <table-separator />

                            { for locales.into_iter().map(|locale| {
                                let selected = self.selected == Some(locale);
                                let (name, flag) = locale_label(locale, "Default Language");

                                html! {
                                    <div key={locale.to_string()} class={classes!("row", "clickable", selected.then_some("active"))} onclick={link.callback(move |_| Msg::Pick(Some(locale)))}>
                                        <span class="fill">{name}</span>

                                        if selected {
                                            <span class="item-inline">
                                                <span class="icon check" />
                                            </span>
                                        }

                                        if let Some(flag) = flag {
                                            <span class={classes!("item-inline", "flag", flag)} title={locale} />
                                        } else {
                                            <span class="item-inline">
                                                <span class="text-muted">{locale}</span>
                                            </span>
                                        }
                                    </div>
                                }
                            }) }
                        </div>
                    </ContextMenu>
                }
            </>
        }
    }
}
