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
            .context("Expected a configured TMDB client")?
            .fetch_image(path)
            .await
    }

    pub(crate) async fn fetch_tvdb_image(&self, path: &str) -> Result<Option<bytes::Bytes>> {
        self.tvdb()
            .context("Expected a configured TVDB client")?
            .fetch_image(path)
            .await
    }

    fn tvmaze(&self) -> Option<crate::tvmaze::Client> {
        self.inner.lock().tvmaze.clone()
    }

    /// Search show across all configured sources (tmdb then tvdb), one page
    /// per source merged together. `already_tracked` is left as `None`; the
    /// caller fills it in from the DB. Returns the results and the total number
    /// of results across the queried sources.
    pub(crate) async fn search_show(
        &self,
        query: &str,
        page: usize,
    ) -> Result<(Vec<api::SearchShow>, usize)> {
        let tmdb = self.tmdb();
        let tvdb = self.tvdb();

        let mut a = Vec::new();
        let mut b = Vec::new();

        let mut total = 0;

        if let Some(client) = tmdb {
            let (results, count) = client.search_show(query, page).await?;
            total += count;

            for r in results {
                a.push(api::SearchShow {
                    remote: r.remote,
                    slug: None,
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
            let (results, count) = client.search_series(query, page).await?;
            total += count;

            for r in results {
                b.push(api::SearchShow {
                    remote: r.remote,
                    slug: r.slug,
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

        let mut out = Vec::new();

        let mut b = b.into_iter();

        for a in a {
            out.push(a);

            if let Some(b) = b.next() {
                out.push(b);
            }
        }

        Ok((out, total))
    }

    /// Search movies (tmdb only). `already_tracked` is left as `None`. Returns
    /// the results and the total number of results.
    pub(crate) async fn search_movies(
        &self,
        query: &str,
        page: usize,
    ) -> Result<(Vec<api::SearchMovie>, usize)> {
        let mut out = Vec::new();
        let mut total = 0;

        if let Some(client) = self.tmdb() {
            let (results, count) = client.search_movies(query, page).await?;
            total += count;

            for r in results {
                out.push(api::SearchMovie {
                    remote: r.remote,
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

        Ok((out, total))
    }

    pub(crate) async fn fetch_tmdb_show(
        &self,
        id: u32,
        language: api::Locale,
    ) -> Result<tmdb::ShowInfo> {
        self.tmdb()
            .context("Expected a configured TMDB client")?
            .fetch_show(id, language)
            .await
    }

    pub(crate) async fn fetch_tmdb_season_episodes(
        &self,
        show_id: u32,
        season: api::SeasonNumber,
        language: api::Locale,
    ) -> Result<Vec<tmdb::EpisodeInfo>> {
        self.tmdb()
            .context("Expected a configured TMDB client")?
            .fetch_season_episodes(show_id, season, language)
            .await
    }

    pub(crate) async fn fetch_tmdb_movie(
        &self,
        id: u32,
        language: api::Locale,
    ) -> Result<tmdb::MovieInfo> {
        self.tmdb()
            .context("Expected a configured TMDB client")?
            .fetch_movie(id, language)
            .await
    }

    pub(crate) async fn fetch_tmdb_movie_releases(
        &self,
        id: u32,
    ) -> Result<Vec<tmdb::MovieReleaseInfo>> {
        self.tmdb()
            .context("Expected a configured TMDB client")?
            .fetch_movie_releases(id)
            .await
    }

    pub(crate) async fn fetch_tvdb_show(
        &self,
        id: u32,
        language: api::Locale,
    ) -> Result<tvdb::SeriesInfo> {
        self.tvdb()
            .context("Expected a configured TVDB client")?
            .fetch_show(id, language)
            .await
    }

    pub(crate) async fn fetch_tvdb_episodes(
        &self,
        show_id: u32,
        language: api::Locale,
    ) -> Result<Vec<tvdb::EpisodeInfo>> {
        self.tvdb()
            .context("Expected a configured TVDB client")?
            .fetch_episodes(show_id, language)
            .await
    }

    pub(crate) async fn lookup_tvmaze_by_tvdb(&self, tvdb_id: u32) -> Result<Option<u32>> {
        self.tvmaze()
            .context("Expected a configured TVmaze client")?
            .lookup_by_tvdb(tvdb_id)
            .await
    }

    pub(crate) async fn lookup_tvmaze_by_imdb(&self, imdb_id: &str) -> Result<Option<u32>> {
        self.tvmaze()
            .context("Expected a configured TVmaze client")?
            .lookup_by_imdb(imdb_id)
            .await
    }

    pub(crate) async fn fetch_tvmaze_episodes(
        &self,
        tvmaze_id: u32,
    ) -> Result<Vec<crate::tvmaze::EpisodeInfo>> {
        self.tvmaze()
            .context("Expected a configured TVmaze client")?
            .fetch_episodes(tvmaze_id)
            .await
    }

    pub(crate) async fn fetch_tvmaze_show_network(
        &self,
        tvmaze_id: u32,
    ) -> Result<crate::tvmaze::ShowNetwork> {
        self.tvmaze()
            .context("Expected a configured TVmaze client")?
            .fetch_show_network(tvmaze_id)
            .await
    }
}
