#![allow(clippy::type_complexity)]

mod app;
mod background;
mod calendar;
mod dashboard;
mod error;
mod image;
mod image_gallery;
mod mark_time_menu;
mod media;
mod modal;
mod movie_detail;
mod outline;
mod queue;
mod root;
mod router;
mod search;
mod settings;
mod setup_channel;
mod show_detail;
mod translations;
mod ui;

use self::app::App;
use self::calendar::Calendar;
use self::dashboard::Dashboard;
use self::image::Image;
use self::image_gallery::{ImageGallery, ImageItem};
use self::media::MediaList;
use self::modal::Modal;
use self::movie_detail::MovieDetail;
use self::queue::Queue;
use self::search::Search;
use self::settings::Settings;
use self::setup_channel::SetupChannel;
use self::show_detail::ShowDetail;
use self::translations::TranslationsModal;

use tracing::Level;
use tracing_wasm::WASMLayerConfigBuilder;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn main() {
    let mut config = WASMLayerConfigBuilder::default();
    config.set_max_level(Level::INFO);
    let config = config.build();
    tracing_wasm::set_as_global_default_with_config(config);
    yew::Renderer::<root::Root>::new().render();
}
