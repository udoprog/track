use std::sync::Arc;

use anyhow::{Context as _, Result};
use parking_lot::Mutex;

/// Holds TMDB and TVDB clients, constructed only when the relevant API key is
/// configured. Call `configure` on startup and whenever `SetConfig` is handled.
#[derive(Clone)]
pub(crate) struct RemoteClients {
    http: reqwest::Client,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    tmdb: Option<crate::tmdb::Client>,
    tvdb: Option<crate::tvdb::Client>,
}

impl RemoteClients {
    pub(crate) fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            inner: Arc::new(Mutex::new(Inner::default())),
        }
    }

    pub(crate) fn configure(&self, config: &api::Config) {
        let mut inner = self.inner.lock();
        inner.tmdb = (!config.tmdb_api_key.is_empty()).then(|| {
            crate::tmdb::Client::new(self.http.clone(), config.tmdb_api_key.clone())
        });
        inner.tvdb = (!config.tvdb_legacy_apikey.is_empty()).then(|| {
            crate::tvdb::Client::new(self.http.clone(), config.tvdb_legacy_apikey.clone())
        });
    }

    fn tmdb(&self) -> Option<crate::tmdb::Client> {
        self.inner.lock().tmdb.clone()
    }

    fn tvdb(&self) -> Option<crate::tvdb::Client> {
        self.inner.lock().tvdb.clone()
    }

    // ── Search ────────────────────────────────────────────────────────────────

    /// Search series across all configured sources (TMDB then TVDB).
    /// `already_tracked` is left as `None`; the caller fills it in from the DB.
    pub(crate) async fn search_series(&self, query: &str) -> Result<Vec<api::SearchSeries>> {
        let tmdb = self.tmdb();
        let tvdb = self.tvdb();
        let mut out = Vec::new();

        if let Some(client) = tmdb {
            for r in client.search_series(query).await? {
                out.push(api::SearchSeries {
                    remote_id: r.remote_id,
                    title: r.title,
                    poster: r.poster,
                    overview: r.overview,
                    first_air_date: r.first_air_date,
                    already_tracked: None,
                });
            }
        }
        if let Some(client) = tvdb {
            for r in client.search_series(query).await? {
                out.push(api::SearchSeries {
                    remote_id: r.remote_id,
                    title: r.title,
                    poster: r.poster,
                    overview: r.overview,
                    first_air_date: r.first_air_date,
                    already_tracked: None,
                });
            }
        }
        Ok(out)
    }

    /// Search movies (TMDB only). `already_tracked` is left as `None`.
    pub(crate) async fn search_movies(&self, query: &str) -> Result<Vec<api::SearchMovie>> {
        let mut out = Vec::new();
        if let Some(client) = self.tmdb() {
            for r in client.search_movies(query).await? {
                out.push(api::SearchMovie {
                    remote_id: r.remote_id,
                    title: r.title,
                    poster: r.poster,
                    overview: r.overview,
                    release_date: r.release_date,
                    already_tracked: None,
                });
            }
        }
        Ok(out)
    }

    // ── Sync fetch helpers ────────────────────────────────────────────────────

    pub(crate) async fn fetch_tmdb_series(
        &self,
        id: u32,
    ) -> Result<crate::tmdb::SeriesInfo> {
        self.tmdb()
            .context("no TMDB client configured")?
            .fetch_series(id)
            .await
    }

    pub(crate) async fn fetch_tmdb_season_episodes(
        &self,
        series_id: u32,
        season: u32,
    ) -> Result<Vec<crate::tmdb::EpisodeInfo>> {
        self.tmdb()
            .context("no TMDB client configured")?
            .fetch_season_episodes(series_id, season)
            .await
    }

    pub(crate) async fn fetch_tmdb_movie(&self, id: u32) -> Result<crate::tmdb::MovieInfo> {
        self.tmdb()
            .context("no TMDB client configured")?
            .fetch_movie(id)
            .await
    }

    pub(crate) async fn fetch_tvdb_series(
        &self,
        id: u32,
    ) -> Result<crate::tvdb::SeriesInfo> {
        self.tvdb()
            .context("no TVDB client configured")?
            .fetch_series(id)
            .await
    }

    pub(crate) async fn fetch_tvdb_episodes(
        &self,
        series_id: u32,
    ) -> Result<Vec<crate::tvdb::EpisodeInfo>> {
        self.tvdb()
            .context("no TVDB client configured")?
            .fetch_episodes(series_id)
            .await
    }
}
