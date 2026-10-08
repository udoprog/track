use yew::prelude::*;

use crate::help;
use crate::ui::Button;

/// Opens the help modal, provided by the app through context.
#[derive(Clone, PartialEq)]
pub(crate) struct HelpControl {
    open: Callback<Option<AttrValue>>,
}

impl HelpControl {
    pub(crate) fn new(open: Callback<Option<AttrValue>>) -> Self {
        Self { open }
    }

    /// Open help at `section`, or at the first section.
    pub(crate) fn open(&self, section: Option<&'static str>) {
        self.open.emit(section.map(AttrValue::Static));
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    /// The id of the help section this control is explained in.
    pub(crate) section: &'static str,
}

/// A question mark beside a control, opening the help section about it.
#[function_component]
pub(crate) fn Help(props: &Props) -> Html {
    let control = use_context::<HelpControl>();
    let section = props.section;

    let title = match help::section(section) {
        Some(found) => format!("Help: {}", found.title),
        None => String::from("Help"),
    };

    let onclick = Callback::from(move |e: MouseEvent| {
        e.stop_propagation();

        if let Some(control) = &control {
            control.open(Some(section));
        }
    });

    html! {
        <Button class="help-mark" icon="question-mark-circle" {title} haspopup="dialog" {onclick} />
    }
}
