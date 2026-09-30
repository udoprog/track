#![allow(clippy::type_complexity)]

mod app;
mod background;
mod error;
mod page;
mod router;
mod setup_channel;
mod theme;
mod ui;

use self::app::App;
use self::setup_channel::SetupChannel;

use tracing::Level;
use tracing_wasm::WASMLayerConfigBuilder;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn main() {
    let mut config = WASMLayerConfigBuilder::default();
    config.set_max_level(Level::INFO);
    let config = config.build();
    tracing_wasm::set_as_global_default_with_config(config);
    yew::Renderer::<App>::new().render();
}
