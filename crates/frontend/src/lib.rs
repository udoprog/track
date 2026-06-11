mod app;
mod calendar;
mod dashboard;
mod error;
mod image;
mod movie_detail;
mod movies;
mod queue;
mod root;
mod router;
mod search;
mod series;
mod series_detail;
mod settings;
mod setup_channel;
mod ui;

use self::app::App;
use self::calendar::Calendar;
use self::dashboard::Dashboard;
use self::image::Image;
use self::movie_detail::MovieDetail;
use self::movies::MoviesList;
use self::queue::Queue;
use self::search::Search;
use self::series::SeriesList;
use self::series_detail::SeriesDetail;
use self::settings::Settings;
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
    yew::Renderer::<root::Root>::new().render();
}
