use std::sync::Arc;

use anyhow::{Context as _, Result};
use api::{Image, ImageKey};
use parking_lot::Mutex;

use crate::{tmdb, tvdb};

/// Choose the primary image for a kind: prefer the API's `selected` image when
/// it's present in the gallery, otherwise fall back to the first entry.
/// `images` is expected to be ordered best-first (highest score), so the
/// fallback is the highest-scored image.
pub(crate) fn best_image(images: &[Image], selected: Option<ImageKey>) -> Option<ImageKey> {
    if let Some(ref selected) = selected
        && let Some(found) = images.iter().find(|image| image.key() == selected)
    {
        return Some(found.key().clone());
    }

    images.first().map(|image| image.key().clone())
}

/// Holds tmdb and tvdb clients, constructed only when the relevant API key is
/// configured. Call `configure` on startup and whenever `SetConfig` is handled.
#[derive(Clone)]
pub(crate) struct RemoteClients {
    http: reqwest::Client,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    tmdb: Option<tmdb::Client>,
    tvdb: Option<tvdb::Client>,
    tvmaze: Option<crate::tvmaze::Client>,
}

impl RemoteClients {
    pub(crate) fn new(http: reqwest::Client) -> Self {
        let inner = Inner {
            tvmaze: Some(crate::tvmaze::Client::new(http.clone())),
            ..Inner::default()
        };
        Self {
            http,
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub(crate) fn configure(&self, config: &api::Config) -> Result<()> {
        let mut inner = self.inner.lock();

        inner.tmdb = if config.tmdb_api_key.is_empty() {
            None
        } else {
            Some(tmdb::Client::new(
                self.http.clone(),
                config.tmdb_api_key.clone(),
            )?)
        };

        inner.tvdb = if config.tvdb_api_key.is_empty() {
            None
        } else {
            Some(tvdb::Client::new(
                self.http.clone(),
                config.tvdb_api_key.clone(),
                config.tvdb_pin.clone(),
            )?)
        };

        if inner.tvmaze.is_none() {
            inner.tvmaze = Some(crate::tvmaze::Client::new(self.http.clone()));
        }

        Ok(())
    }

    fn tmdb(&self) -> Option<tmdb::Client> {
        self.inner.lock().tmdb.clone()
    }

    fn tvdb(&self) -> Option<tvdb::Client> {
        self.inner.lock().tvdb.clone()
    }

    pub(crate) async fn fetch_tmdb_image(&self, path: &str) -> Result<Option<bytes::Bytes>> {
        self.tmdb()
            .context("no tmdb client configured")?
            .fetch_image(path)
            .await
    }

    pub(crate) async fn fetch_tvdb_image(&self, path: &str) -> Result<Option<bytes::Bytes>> {
        self.tvdb()
            .context("no tvdb client configured")?
            .fetch_image(path)
            .await
    }

    fn tvmaze(&self) -> Option<crate::tvmaze::Client> {
        self.inner.lock().tvmaze.clone()
    }

    // ── Search ────────────────────────────────────────────────────────────────

    /// Search series across all configured sources (tmdb then tvdb), one page
    /// per source merged together. `already_tracked` is left as `None`; the
    /// caller fills it in from the DB. Returns the results and whether a further
    /// page can be fetched from any source.
    pub(crate) async fn search_series(
        &self,
        query: &str,
        page: usize,
    ) -> Result<(Vec<api::SearchSeries>, bool)> {
        let tmdb = self.tmdb();
        let tvdb = self.tvdb();

        let mut out = Vec::new();
        let mut has_more = false;

        if let Some(client) = tmdb {
            let (results, pages) = client.search_series(query, page).await?;
            has_more |= page + 1 < pages;

            for r in results {
                out.push(api::SearchSeries {
                    remote_id: r.remote_id,
                    title: r.title,
                    poster: r.poster.clone().map(api::Image::from),
                    banner: r.backdrop.clone().map(api::Image::from),
                    backdrop: r.backdrop.clone().map(api::Image::from),
                    overview: r.overview,
                    first_air_date: r.first_air_date,
                    already_tracked: None,
                });
            }
        }

        if let Some(client) = tvdb {
            let (results, pages) = client.search_series(query, page).await?;
            has_more |= page + 1 < pages;

            for r in results {
                out.push(api::SearchSeries {
                    remote_id: r.remote_id,
                    title: r.title,
                    poster: r
                        .poster
                        .map(|(source, path)| api::Image::new(source, &path)),
                    banner: r
                        .banner
                        .map(|(source, path)| api::Image::new(source, &path)),
                    backdrop: r
                        .fanart
                        .map(|(source, path)| api::Image::new(source, &path)),
                    overview: r.overview,
                    first_air_date: r.first_air_date,
                    already_tracked: None,
                });
            }
        }

        Ok((out, has_more))
    }

    /// Search movies (tmdb only). `already_tracked` is left as `None`. Returns
    /// the results and whether a further page can be fetched.
    pub(crate) async fn search_movies(
        &self,
        query: &str,
        page: usize,
    ) -> Result<(Vec<api::SearchMovie>, bool)> {
        let mut out = Vec::new();
        let mut has_more = false;

        if let Some(client) = self.tmdb() {
            let (results, pages) = client.search_movies(query, page).await?;
            has_more |= page + 1 < pages;

            for r in results {
                out.push(api::SearchMovie {
                    remote_id: r.remote_id,
                    title: r.title,
                    poster: r.poster.clone().map(api::Image::from),
                    banner: r.backdrop.clone().map(api::Image::from),
                    backdrop: r.backdrop.clone().map(api::Image::from),
                    overview: r.overview,
                    release_date: r.release_date,
                    already_tracked: None,
                });
            }
        }

        Ok((out, has_more))
    }

    // ── Sync fetch helpers ────────────────────────────────────────────────────

    pub(crate) async fn fetch_tmdb_series(
        &self,
        id: u32,
        language: Option<&str>,
    ) -> Result<tmdb::SeriesInfo> {
        self.tmdb()
            .context("no tmdb client configured")?
            .fetch_series(id, language)
            .await
    }

    pub(crate) async fn fetch_tmdb_season_episodes(
        &self,
        series_id: u32,
        season: api::SeasonNumber,
        language: Option<&str>,
    ) -> Result<Vec<tmdb::EpisodeInfo>> {
        self.tmdb()
            .context("no tmdb client configured")?
            .fetch_season_episodes(series_id, season, language)
            .await
    }

    pub(crate) async fn fetch_tmdb_movie(
        &self,
        id: u32,
        language: Option<&str>,
    ) -> Result<tmdb::MovieInfo> {
        self.tmdb()
            .context("no tmdb client configured")?
            .fetch_movie(id, language)
            .await
    }

    pub(crate) async fn fetch_tmdb_movie_releases(
        &self,
        id: u32,
    ) -> Result<Vec<tmdb::MovieReleaseInfo>> {
        self.tmdb()
            .context("no tmdb client configured")?
            .fetch_movie_releases(id)
            .await
    }

    pub(crate) async fn fetch_tvdb_series(
        &self,
        id: u32,
        language: Option<&str>,
    ) -> Result<tvdb::SeriesInfo> {
        self.tvdb()
            .context("no tvdb client configured")?
            .fetch_series(id, language)
            .await
    }

    pub(crate) async fn fetch_tvdb_episodes(
        &self,
        series_id: u32,
        language: Option<&str>,
    ) -> Result<Vec<tvdb::EpisodeInfo>> {
        self.tvdb()
            .context("no tvdb client configured")?
            .fetch_episodes(series_id, language)
            .await
    }

    pub(crate) async fn lookup_tvmaze_by_tvdb(&self, tvdb_id: u32) -> Result<Option<u32>> {
        self.tvmaze()
            .context("no tvmaze client")?
            .lookup_by_tvdb(tvdb_id)
            .await
    }

    pub(crate) async fn lookup_tvmaze_by_imdb(&self, imdb_id: &str) -> Result<Option<u32>> {
        self.tvmaze()
            .context("no tvmaze client")?
            .lookup_by_imdb(imdb_id)
            .await
    }

    pub(crate) async fn fetch_tvmaze_episodes(
        &self,
        tvmaze_id: u32,
    ) -> Result<Vec<crate::tvmaze::EpisodeInfo>> {
        self.tvmaze()
            .context("no tvmaze client")?
            .fetch_episodes(tvmaze_id)
            .await
    }
}
