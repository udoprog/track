use core::time::Duration;
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use api::{Image, ImageKey};
use parking_lot::Mutex;

use crate::{tmdb, tvdb};

/// Choose the primary image for a kind: prefer the API's `selected` image when
/// it's present in the gallery, otherwise fall back to the first entry.
/// `images` is expected to be ordered best-first (highest score), so the
/// fallback is the highest-scored image.
pub(crate) fn best_image(images: &[(f64, Image)], selected: Option<ImageKey>) -> Option<ImageKey> {
    if let Some(ref selected) = selected
        && let Some((_, found)) = images.iter().find(|(_, image)| image.key() == selected)
    {
        return Some(found.key().clone());
    }

    images.first().map(|(_, image)| image.key().clone())
}

/// Whether `path` is relative and made only of plain segments, so that it can
/// neither replace the image base when joined nor escape the cache directory.
pub(crate) fn is_plain_image_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.contains([':', '\\'])
                && !segment.contains(char::is_control)
        })
}

/// Join an image path onto `base`, refusing any URL outside the base.
pub(crate) fn join_image_url(base: &reqwest::Url, path: &str) -> Result<reqwest::Url> {
    ensure!(is_plain_image_path(path), "Invalid image path {path:?}");
    let url = base.join(path)?;
    ensure!(
        url.scheme() == base.scheme()
            && url.host() == base.host()
            && url.port_or_known_default() == base.port_or_known_default()
            && url.path().starts_with(base.path()),
        "Image path {path:?} resolves outside {base}"
    );
    Ok(url)
}

/// Holds tmdb and tvdb clients, constructed only when the relevant API key is
/// configured. Call `configure` on startup and whenever `SetConfig` is handled.
#[derive(Clone)]
pub(crate) struct RemoteClients {
    http: reqwest::Client,
    rate_limiter: Arc<leaky_bucket::RateLimiter>,
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

        let rate_limiter = leaky_bucket::RateLimiter::builder()
            .initial(10)
            .interval(Duration::from_millis(100))
            .refill(1)
            .build();

        Self {
            http,
            rate_limiter: Arc::new(rate_limiter),
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

    async fn tmdb(&self) -> Option<tmdb::Client> {
        let tmdb = self.inner.lock().tmdb.clone()?;
        self.rate_limiter.acquire_one().await;
        Some(tmdb)
    }

    async fn tvdb(&self) -> Option<tvdb::Client> {
        let tvdb = self.inner.lock().tvdb.clone()?;
        self.rate_limiter.acquire_one().await;
        Some(tvdb)
    }

    // Images come from the CDNs, so they skip the API rate limiter.
    pub(crate) async fn fetch_tmdb_image(&self, path: &str) -> Result<Option<bytes::Bytes>> {
        let tmdb = self.inner.lock().tmdb.clone();
        tmdb.context("Expected a configured TMDB client")?
            .fetch_image(path)
            .await
    }

    pub(crate) async fn fetch_tvdb_image(&self, path: &str) -> Result<Option<bytes::Bytes>> {
        let tvdb = self.inner.lock().tvdb.clone();
        tvdb.context("Expected a configured TVDB client")?
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
        let tmdb = self.tmdb().await;
        let tvdb = self.tvdb().await;

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

        if let Some(client) = self.tmdb().await {
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
        etag: Option<&str>,
    ) -> Result<tmdb::Conditional<tmdb::ShowInfo>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_show(id, etag)
            .await
    }

    pub(crate) async fn fetch_tmdb_season_episodes(
        &self,
        show_id: u32,
        season: api::SeasonNumber,
    ) -> Result<Vec<tmdb::EpisodeInfo>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_season_episodes(show_id, season)
            .await
    }

    pub(crate) async fn fetch_tmdb_episode(
        &self,
        show_id: u32,
        season: api::SeasonNumber,
        number: u32,
        etag: Option<&str>,
    ) -> Result<tmdb::Conditional<tmdb::EpisodeInfo>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_episode(show_id, season, number, etag)
            .await
    }

    pub(crate) async fn fetch_tmdb_movie(
        &self,
        id: u32,
        etag: Option<&str>,
    ) -> Result<tmdb::Conditional<tmdb::MovieInfo>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_movie(id, etag)
            .await
    }

    pub(crate) async fn fetch_tmdb_movie_releases(
        &self,
        id: u32,
    ) -> Result<Vec<tmdb::MovieReleaseInfo>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_movie_releases(id)
            .await
    }

    pub(crate) async fn fetch_tmdb_show_translations(
        &self,
        id: u32,
    ) -> Result<Vec<tmdb::Translation>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_show_translations(id)
            .await
    }

    pub(crate) async fn fetch_tmdb_person(
        &self,
        id: u32,
        etag: Option<&str>,
    ) -> Result<tmdb::Conditional<tmdb::PersonInfo>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_person(id, etag)
            .await
    }

    pub(crate) async fn fetch_tmdb_person_translations(
        &self,
        id: u32,
    ) -> Result<Vec<tmdb::PersonTranslation>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_person_translations(id)
            .await
    }

    pub(crate) async fn fetch_tmdb_person_images(&self, id: u32) -> Result<Vec<(f64, api::Image)>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_person_images(id)
            .await
    }

    pub(crate) async fn fetch_tmdb_show_credits(
        &self,
        id: u32,
        language: &str,
    ) -> Result<Vec<tmdb::CreditInfo>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_show_credits(id, language)
            .await
    }

    pub(crate) async fn fetch_tmdb_movie_credits(
        &self,
        id: u32,
        language: &str,
    ) -> Result<Vec<tmdb::CreditInfo>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_movie_credits(id, language)
            .await
    }

    pub(crate) async fn fetch_tmdb_season_translations(
        &self,
        show_id: u32,
        season: api::SeasonNumber,
    ) -> Result<Vec<tmdb::Translation>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_season_translations(show_id, season)
            .await
    }

    pub(crate) async fn fetch_tmdb_episode_translations(
        &self,
        show_id: u32,
        season: api::SeasonNumber,
        episode: u32,
    ) -> Result<Vec<tmdb::Translation>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_episode_translations(show_id, season, episode)
            .await
    }

    pub(crate) async fn fetch_tmdb_movie_translations(
        &self,
        tvdb_id: u32,
    ) -> Result<Vec<tmdb::Translation>> {
        self.tmdb()
            .await
            .context("Expected a configured TMDB client")?
            .fetch_movie_translations(tvdb_id)
            .await
    }

    pub(crate) async fn fetch_tvdb_show(&self, tvdb_id: u32) -> Result<tvdb::SeriesInfo> {
        self.tvdb()
            .await
            .context("Expected a configured TVDB client")?
            .fetch_show(tvdb_id)
            .await
    }

    pub(crate) async fn fetch_tvdb_episodes(&self, tvdb_id: u32) -> Result<Vec<tvdb::EpisodeInfo>> {
        self.tvdb()
            .await
            .context("Expected a configured TVDB client")?
            .fetch_episodes(tvdb_id)
            .await
    }

    pub(crate) async fn fetch_tvdb_episode(
        &self,
        tvdb_id: u32,
        season: api::SeasonNumber,
        number: u32,
    ) -> Result<Option<tvdb::EpisodeInfo>> {
        self.tvdb()
            .await
            .context("Expected a configured TVDB client")?
            .fetch_episode(tvdb_id, season, number)
            .await
    }

    pub(crate) async fn fetch_tvdb_show_translation(
        &self,
        tvdb_id: u32,
        language: api::Locale,
        available: &HashSet<String>,
    ) -> Result<Option<tvdb::Translation>> {
        self.tvdb()
            .await
            .context("Expected a configured TVDB client")?
            .fetch_show_translation(tvdb_id, language, available)
            .await
    }

    pub(crate) async fn fetch_tvdb_season_translation(
        &self,
        season_id: u32,
        language: api::Locale,
        available: &HashSet<String>,
    ) -> Result<Option<tvdb::Translation>> {
        self.tvdb()
            .await
            .context("Expected a configured TVDB client")?
            .fetch_season_translation(season_id, language, available)
            .await
    }

    pub(crate) async fn fetch_tvdb_episode_translation(
        &self,
        episode_id: u32,
        language: api::Locale,
        available: &HashSet<String>,
    ) -> Result<Option<tvdb::Translation>> {
        self.tvdb()
            .await
            .context("Expected a configured TVDB client")?
            .fetch_episode_translation(episode_id, language, available)
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

    pub(crate) async fn fetch_tvmaze_episode(
        &self,
        tvmaze_id: u32,
        season: api::SeasonNumber,
        number: u32,
    ) -> Result<Option<crate::tvmaze::EpisodeInfo>> {
        self.tvmaze()
            .context("Expected a configured TVmaze client")?
            .fetch_episode(tvmaze_id, season, number)
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

#[cfg(test)]
mod tests {
    use super::{is_plain_image_path, join_image_url};

    const BAD: &[&str] = &[
        "",
        "https://evil.example/x",
        "http:169.254.169.254/latest/meta-data",
        "//evil.example/x",
        "a//b.jpg",
        "/abc.jpg",
        "../abc.jpg",
        "a/../../abc.jpg",
        "./abc.jpg",
        "\\\\evil.example/x",
        "a\\..\\b.jpg",
        "/\t/evil.example/x",
        "\t//evil.example/x",
    ];

    #[test]
    fn plain_image_paths() {
        assert!(is_plain_image_path("abc.jpg"));
        assert!(is_plain_image_path(
            "banners/v4/series/81189/posters/5f.jpg"
        ));
        assert!(is_plain_image_path("a..b.jpg"));

        for path in BAD {
            assert!(!is_plain_image_path(path), "{path:?}");
        }
    }

    #[test]
    fn join_stays_under_base() {
        let base = reqwest::Url::parse("https://image.tmdb.org/t/p/original/").unwrap();

        assert_eq!(
            join_image_url(&base, "abc.jpg").unwrap().as_str(),
            "https://image.tmdb.org/t/p/original/abc.jpg"
        );

        for path in BAD.iter().chain(&["%2e%2e/abc.jpg", "%2E%2e/%2e%2e/x.jpg"]) {
            assert!(join_image_url(&base, path).is_err(), "{path:?}");
        }
    }
}
