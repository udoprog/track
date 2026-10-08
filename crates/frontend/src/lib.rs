#![allow(clippy::type_complexity)]

mod active_tasks;
mod app;
mod background;
mod error;
mod help;
mod http;
mod page;
mod root;
mod router;
mod setup_channel;
mod theme;
mod ui;

use self::root::Root;
use self::setup_channel::SetupChannel;

use tracing::Level;
use tracing_wasm::WASMLayerConfigBuilder;
use wasm_bindgen::prelude::*;

/// Whether the signed-in user is an administrator, so components can leave out
/// actions only administrators may take.
fn is_admin<C>(ctx: &yew::Context<C>) -> bool
where
    C: yew::Component,
{
    ctx.link()
        .context::<api::User>(yew::Callback::noop())
        .is_some_and(|(user, _)| user.role == api::UserRole::Admin)
}

#[wasm_bindgen(start)]
fn main() {
    let mut config = WASMLayerConfigBuilder::default();
    config.set_max_level(Level::INFO);
    let config = config.build();
    tracing_wasm::set_as_global_default_with_config(config);
    yew::Renderer::<Root>::new().render();
}
