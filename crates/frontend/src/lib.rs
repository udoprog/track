#![allow(clippy::type_complexity)]

mod active_tasks;
mod app;
mod background;
// Unused constants are fine: the dead-class guard (tests/style_classes.rs)
// checks the classes, and the help pages name icons at runtime.
#[allow(dead_code)]
mod class {
    include!(concat!(env!("OUT_DIR"), "/classes.rs"));
}
mod error;
mod flags;
mod help;
mod l10n;
mod page;
mod root;
mod router;
mod theme;
mod ui;

use self::root::Root;

use tracing::Level;
use tracing_wasm::WASMLayerConfigBuilder;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn main() {
    let mut config = WASMLayerConfigBuilder::default();
    config.set_max_level(Level::INFO);
    let config = config.build();
    tracing_wasm::set_as_global_default_with_config(config);
    yew::Renderer::<Root>::new().render();
}
