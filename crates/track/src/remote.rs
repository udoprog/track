use core::time::Duration;
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use api::{Image, ImageKey};
use parking_lot::Mutex;

use crate::{tmdb, tvdb, tvmaze, xem};

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

/// Fetch an image from `url` with a client that does not follow redirects.
/// A missing image is `None`; a redirect or any other failure is an error.
pub(crate) async fn fetch_image_bytes(
    http: &reqwest::Client,
    url: reqwest::Url,
) -> Result<Option<bytes::Bytes>> {
    let resp = http.get(url).send().await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    ensure!(
        !resp.status().is_redirection(),
        "Unexpected redirect ({}) fetching image",
        resp.status()
    );

    Ok(Some(resp.error_for_status()?.bytes().await?))
}

/// Forward each listed method to the client `$client` returns, failing with
/// `$missing` when it is not configured.
macro_rules! forward {
    ($client:ident, $missing:literal; $(fn $name:ident = $method:ident($($arg:ident: $ty:ty),*) -> $ret:ty;)*) => {
        $(
            pub(crate) async fn $name(&self, $($arg: $ty),*) -> Result<$ret> {
                self.$client().await.context($missing)?.$method($($arg),*).await
            }
        )*
    };
}

/// Holds tmdb and tvdb clients, constructed only when the relevant API key is
/// configured. Call `configure` on startup and whenever `SetConfig` is handled.
#[derive(Clone)]
pub(crate) struct RemoteClients {
    http: reqwest::Client,
    image_http: reqwest::Client,
    rate_limiter: Arc<leaky_bucket::RateLimiter>,
    inner: Arc<Mutex<Inner>>,
    pub(crate) xem: xem::Client,
}

#[derive(Default)]
struct Inner {
    tmdb: Option<tmdb::Client>,
    tvdb: Option<tvdb::Client>,
    tvmaze: Option<crate::tvmaze::Client>,
}

impl RemoteClients {
    pub(crate) fn new(http: reqwest::Client, image_http: reqwest::Client) -> Self {
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
            xem: xem::Client::new(http.clone()),
            http,
            image_http,
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
                self.image_http.clone(),
                config.tmdb_api_key.clone(),
            )?)
        };

        inner.tvdb = if config.tvdb_api_key.is_empty() {
            None
        } else {
            Some(tvdb::Client::new(
                self.http.clone(),
                self.image_http.clone(),
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

    async fn tvmaze(&self) -> Option<tvmaze::Client> {
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

        let out = interleave(a, b);

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

    forward! {
        tmdb, "Expected a configured TMDB client";
        fn fetch_tmdb_show = fetch_show(id: u32, etag: Option<&str>) -> tmdb::Conditional<tmdb::ShowInfo>;
        fn fetch_tmdb_season_episodes = fetch_season_episodes(show_id: u32, season: api::SeasonNumber) -> Vec<tmdb::EpisodeInfo>;
        fn fetch_tmdb_episode = fetch_episode(show_id: u32, season: api::SeasonNumber, number: u32, etag: Option<&str>) -> tmdb::Conditional<tmdb::EpisodeInfo>;
        fn fetch_tmdb_movie = fetch_movie(id: u32, etag: Option<&str>) -> tmdb::Conditional<tmdb::MovieInfo>;
        fn fetch_tmdb_movie_releases = fetch_movie_releases(id: u32) -> Vec<tmdb::MovieReleaseInfo>;
        fn fetch_tmdb_show_translations = fetch_show_translations(id: u32) -> Vec<tmdb::Translation>;
        fn fetch_tmdb_person = fetch_person(id: u32, etag: Option<&str>) -> tmdb::Conditional<tmdb::PersonInfo>;
        fn fetch_tmdb_person_translations = fetch_person_translations(id: u32) -> Vec<tmdb::PersonTranslation>;
        fn fetch_tmdb_person_images = fetch_person_images(id: u32) -> Vec<(f64, api::Image)>;
        fn fetch_tmdb_show_credits = fetch_show_credits(id: u32, language: &str) -> Vec<tmdb::CreditInfo>;
        fn fetch_tmdb_movie_credits = fetch_movie_credits(id: u32, language: &str) -> Vec<tmdb::CreditInfo>;
        fn fetch_tmdb_season_translations = fetch_season_translations(show_id: u32, season: api::SeasonNumber) -> Vec<tmdb::Translation>;
        fn fetch_tmdb_episode_translations = fetch_episode_translations(show_id: u32, season: api::SeasonNumber, episode: u32) -> Vec<tmdb::Translation>;
        fn fetch_tmdb_movie_translations = fetch_movie_translations(id: u32) -> Vec<tmdb::Translation>;
    }

    forward! {
        tvdb, "Expected a configured TVDB client";
        fn fetch_tvdb_show = fetch_show(tvdb_id: u32) -> tvdb::SeriesInfo;
        fn fetch_tvdb_episodes = fetch_episodes(tvdb_id: u32) -> Vec<tvdb::EpisodeInfo>;
        fn fetch_tvdb_episode = fetch_episode(tvdb_id: u32, season: api::SeasonNumber, number: u32) -> Option<tvdb::EpisodeInfo>;
        fn fetch_tvdb_show_translation = fetch_show_translation(tvdb_id: u32, language: api::Locale, available: &HashSet<String>) -> Option<tvdb::Translation>;
        fn fetch_tvdb_season_translation = fetch_season_translation(season_id: u32, language: api::Locale, available: &HashSet<String>) -> Option<tvdb::Translation>;
        fn fetch_tvdb_episode_translation = fetch_episode_translation(episode_id: u32, language: api::Locale, available: &HashSet<String>) -> Option<tvdb::Translation>;
    }

    forward! {
        tvmaze, "Expected a configured TVmaze client";
        fn lookup_tvmaze_by_tvdb = lookup_by_tvdb(tvdb_id: u32) -> Option<u32>;
        fn lookup_tvmaze_by_imdb = lookup_by_imdb(imdb_id: &str) -> Option<u32>;
        fn fetch_tvmaze_episodes = fetch_episodes(tvmaze_id: u32) -> Vec<tvmaze::EpisodeInfo>;
        fn fetch_tvmaze_episode = fetch_episode(tvmaze_id: u32, season: api::SeasonNumber, number: u32) -> Option<tvmaze::EpisodeInfo>;
        fn fetch_tvmaze_show_network = fetch_show_network(tvmaze_id: u32) -> tvmaze::ShowNetwork;
    }
}

/// Alternate items from `a` and `b`, then append whatever is left of the longer.
pub(crate) fn interleave<T>(a: Vec<T>, b: Vec<T>) -> Vec<T> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let mut a = a.into_iter();
    let mut b = b.into_iter();

    loop {
        let x = a.next();
        let y = b.next();

        if x.is_none() && y.is_none() {
            return out;
        }

        out.extend(x);
        out.extend(y);
    }
}

#[cfg(test)]
mod tests {
    use super::{fetch_image_bytes, interleave, is_plain_image_path, join_image_url};

    #[test]
    fn interleave_keeps_leftovers_of_either_side() {
        assert_eq!(interleave(vec![1, 3], vec![2, 4, 6, 8]), [1, 2, 3, 4, 6, 8]);
        assert_eq!(interleave(vec![1, 3, 5, 7], vec![2]), [1, 2, 3, 5, 7]);
        assert_eq!(interleave(Vec::new(), vec![2, 4]), [2, 4]);
        assert_eq!(interleave(vec![1], Vec::new()), [1]);
    }

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

    #[tokio::test]
    async fn image_redirect_is_an_error() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            _ = s.read(&mut buf).await;
            s.write_all(
                b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        });

        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let url = reqwest::Url::parse(&format!("http://{addr}/a.jpg")).unwrap();
        assert!(fetch_image_bytes(&http, url).await.is_err());
    }
}
