use web_sys::MouseEvent;
use yew::prelude::*;

use crate::active_tasks::{ActiveTasks, SyncTarget};
use crate::class;
use crate::router::{QueueQuery, Route};
use crate::ui::Button;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) target: SyncTarget,
    /// Label shown on mobile only, as [`Button`]'s `text`.
    #[prop_or_default]
    pub(crate) text: Option<AttrValue>,
    /// Label shown at every width, as [`Button`]'s `label`.
    #[prop_or_default]
    pub(crate) label: Option<AttrValue>,
    /// Spin before the queue reports the requested sync's task.
    #[prop_or_default]
    pub(crate) requested: bool,
    pub(crate) onclick: Callback<MouseEvent>,
}

/// Syncs `target`, or, while a sync of it is queued or running, spins and
/// links to that task in the queue.
#[function_component]
pub(crate) fn SyncButton(props: &Props) -> Html {
    let tasks = use_context::<ActiveTasks>().unwrap_or_default();

    let Some(task) = tasks.task(props.target) else {
        return html! {
            <Button icon={class::ICON_ARROW_PATH} spin={props.requested} title="Sync now" text={props.text.clone()} label={props.label.clone()} onclick={props.onclick.clone()} />
        };
    };

    let to = Route::Queue(QueueQuery {
        task: Some(task),
        ..QueueQuery::default()
    });

    let syncing = AttrValue::Static("Syncing");

    html! {
        <Button {to} icon={class::ICON_ARROW_PATH} spin=true title="Syncing, show in queue" label={props.label.is_some().then(|| syncing.clone())} text={props.text.is_some().then_some(syncing)} />
    }
}
