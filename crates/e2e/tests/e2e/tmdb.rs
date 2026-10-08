//! A stand-in for the TMDB API that answers every request slowly, so a show
//! sync runs long enough to watch its progress. The show has [`SEASONS`]
//! seasons of [`EPISODES`] episodes; everything else is empty.

use std::time::Duration;

use anyhow::Result;
use axum::http::Uri;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

const DELAY: Duration = Duration::from_millis(250);
const SEASONS: u32 = 2;
const EPISODES: u32 = 8;

/// A running stand-in, stopped when dropped.
pub(crate) struct SlowTmdb {
    pub(crate) base: String,
    task: JoinHandle<()>,
}

impl SlowTmdb {
    pub(crate) async fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base = format!("http://{}/3/", listener.local_addr()?);
        let app = axum::Router::new().fallback(respond);

        let task = tokio::spawn(async move {
            _ = axum::serve(listener, app).await;
        });

        Ok(Self { base, task })
    }
}

impl Drop for SlowTmdb {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn respond(uri: Uri) -> ([(&'static str, &'static str); 1], String) {
    tokio::time::sleep(DELAY).await;

    let parts = uri.path().split('/').skip(2).collect::<Vec<_>>();

    let body = match parts.as_slice() {
        ["tv", _] => {
            let seasons = (1..=SEASONS)
                .map(|n| format!(r#"{{"season_number":{n}}}"#))
                .collect::<Vec<_>>()
                .join(",");

            format!(r#"{{"original_name":"Seeded Show","seasons":[{seasons}]}}"#)
        }
        ["tv", _, "season", _] => {
            let episodes = (1..=EPISODES)
                .map(|n| format!(r#"{{"episode_number":{n},"name":"Episode {n}"}}"#))
                .collect::<Vec<_>>()
                .join(",");

            format!(r#"{{"episodes":[{episodes}]}}"#)
        }
        _ => String::from("{}"),
    };

    ([("content-type", "application/json")], body)
}
