use std::sync::Arc;

use anyhow::{Context as _, Result};
use api::{Date, Image, ImageKey, ImageSource, ReleaseType, Remote, SeasonNumber, Timestamp};
use reqwest::{Method, RequestBuilder};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::remote::best_image;

const BASE: &str = "https://api.themoviedb.org/3/";
const IMAGE_BASE: &str = "https://image.tmdb.org/t/p/original/";

struct Inner {
    base: reqwest::Url,
    image_base: reqwest::Url,
    http: reqwest::Client,
    api_key: String,
}

#[derive(Clone)]
pub(crate) struct Client {
    inner: Arc<Inner>,
}

impl Client {
    pub(crate) fn new(http: reqwest::Client, api_key: String) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                base: reqwest::Url::parse(BASE)?,
                image_base: reqwest::Url::parse(IMAGE_BASE)?,
                http,
                api_key,
            }),
        })
    }

    fn request(&self, method: Method, path: impl AsRef<str>) -> Result<RequestBuilder> {
        let url = self.inner.base.join(path.as_ref())?;

        let req = self
            .inner
            .http
            .request(method, url)
            .query(&[("api_key", self.inner.api_key.as_str())]);

        Ok(req)
    }

    #[tracing::instrument(skip(self, url))]
    async fn get_json<T>(&self, url: impl AsRef<str>, language: api::Locale) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut req = self.request(Method::GET, url.as_ref())?;

        if !language.is_default() {
            req = req.query(&[("language", language)]);
        }

        Self::send_json(req).await
    }

    #[tracing::instrument(skip(self, url))]
    async fn get_images<T>(&self, url: impl AsRef<str>, language: api::Locale) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut req = self.request(Method::GET, url.as_ref())?;
        req = req.query(&[("language", language.or(api::Locale::EN_US))]);
        Self::send_json(req).await
    }

    async fn send_json<T>(req: RequestBuilder) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let bytes = req
            .send()
            .await
            .context("Sending request")?
            .error_for_status()
            .context("Bad response status")?
            .bytes()
            .await
            .context("Reading response body")?;

        serde_json::from_slice(&bytes).context("Deserializing JSON response")
    }

    pub(crate) async fn fetch_image(&self, path: &str) -> Result<Option<bytes::Bytes>> {
        let url = self.inner.image_base.join(path)?;
        let resp = self.inner.http.get(url).send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        Ok(Some(resp.error_for_status()?.bytes().await?))
    }

    /// Search show, returning the results for `page` and the total number of
    /// results across all pages.
    pub(crate) async fn search_show(
        &self,
        query: &str,
        page: usize,
    ) -> Result<(Vec<SearchShowResult>, usize)> {
        #[derive(Deserialize)]
        struct Row {
            id: u32,
            #[serde(default)]
            original_name: Option<String>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            poster_path: Option<String>,
            #[serde(default)]
            backdrop_path: Option<String>,
            #[serde(default)]
            first_air_date: Option<String>,
        }

        #[derive(Deserialize)]
        struct Resp {
            results: Vec<Row>,
            #[serde(default)]
            total_results: usize,
        }

        // TMDB pages are 1-indexed.
        let page = (page + 1).to_string();

        let bytes = self
            .request(Method::GET, "search/tv")
            .context("Building request")?
            .query(&[("query", query), ("page", page.as_str())])
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let resp: Resp = serde_json::from_slice(&bytes)?;

        let mut out = Vec::with_capacity(resp.results.len());

        for r in resp.results {
            out.push(SearchShowResult {
                remote: Remote::tmdb(r.id),
                title: r.name.or(r.original_name),
                overview: r.overview.filter(|s| !s.trim().is_empty()),
                first_air_date: opt_date(r.first_air_date.as_deref()),
                poster: r.poster_path.as_deref().map(ImageKey::tmdb),
                backdrop: r.backdrop_path.as_deref().map(ImageKey::tmdb),
            });
        }

        Ok((out, resp.total_results))
    }

    /// Search movies, returning the results for `page` and the total number of
    /// results across all pages.
    pub(crate) async fn search_movies(
        &self,
        query: &str,
        page: usize,
    ) -> Result<(Vec<SearchMovieResult>, usize)> {
        #[derive(Deserialize)]
        struct Row {
            id: u32,
            #[serde(default)]
            original_title: Option<String>,
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            poster_path: Option<String>,
            #[serde(default)]
            backdrop_path: Option<String>,
            #[serde(default)]
            release_date: Option<String>,
        }

        #[derive(Deserialize)]
        struct Resp {
            results: Vec<Row>,
            #[serde(default)]
            total_results: usize,
        }

        // TMDB pages are 1-indexed.
        let page = (page + 1).to_string();

        let bytes = self
            .request(Method::GET, "search/movie")
            .context("Building request")?
            .query(&[("query", query), ("page", page.as_str())])
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let resp: Resp = serde_json::from_slice(&bytes)?;

        let mut out = Vec::with_capacity(resp.results.len());

        for r in resp.results {
            out.push(SearchMovieResult {
                remote: Remote::tmdb(r.id),
                title: r.title.or(r.original_title),
                overview: r.overview.filter(|s| !s.trim().is_empty()),
                release_date: opt_date(r.release_date.as_deref()),
                poster: r.poster_path.as_deref().map(ImageKey::tmdb),
                backdrop: r.backdrop_path.as_deref().map(ImageKey::tmdb),
            });
        }

        Ok((out, resp.total_results))
    }

    pub(crate) async fn fetch_show(&self, id: u32, language: api::Locale) -> Result<ShowInfo> {
        #[derive(Deserialize)]
        struct SeasonDetails {
            #[serde(default)]
            season_number: Option<u32>,
            #[serde(default)]
            air_date: Option<String>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            poster_path: Option<String>,
        }

        #[derive(Deserialize, Default)]
        struct ExternalIds {
            #[serde(default)]
            tvdb_id: Option<u32>,
            #[serde(default)]
            imdb_id: Option<String>,
        }

        #[derive(Deserialize)]
        struct Details {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            original_name: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            poster_path: Option<String>,
            #[serde(default)]
            backdrop_path: Option<String>,
            #[serde(default)]
            first_air_date: Option<String>,
            #[serde(default)]
            original_language: api::Locale,
            #[serde(default)]
            seasons: Vec<SeasonDetails>,
            #[serde(default)]
            external_ids: ExternalIds,
        }

        let details: Details = self
            .get_json(format!("tv/{id}?append_to_response=external_ids"), language)
            .await?;

        // When no language is configured, fall back to the show's own original language.
        let effective_language = language.or(details
            .original_language
            .filter(|l| l != api::Locale::EN_US));

        // Re-fetch for a localized title and overview when the effective language
        // differs from what was used for the initial request (i.e., no language was
        // configured but the show has a non-English original language).
        let localized: Option<Details> = if effective_language != language {
            self.get_json(
                format!("tv/{id}?append_to_response=external_ids"),
                effective_language,
            )
            .await
            .ok()
        } else {
            None
        };

        let images: Images = self
            .get_images(format!("tv/{id}/images"), effective_language)
            .await
            .context("Fetching images")?;

        // Extract fields used for image selection before consuming details.
        let original_language = details.original_language;
        let poster_path = details.poster_path;
        let backdrop_path = details.backdrop_path;
        let first_air_date = details.first_air_date;

        let mut seasons = Vec::with_capacity(details.seasons.len());

        for s in details.seasons {
            // Prefer the localized season name/overview, falling back to
            // whatever the default-language request returned the same
            // resolution applied to the show title/overview below. Reuses the
            // already-fetched `localized` response.
            let localized_season = localized.as_ref().and_then(|l| {
                l.seasons
                    .iter()
                    .find(|ls| ls.season_number == s.season_number)
            });

            let name = localized_season
                .and_then(|ls| ls.name.as_deref())
                .filter(|s| !s.trim().is_empty())
                .or(s.name.as_deref().filter(|s| !s.trim().is_empty()))
                .map(str::to_owned);

            let overview = localized_season
                .and_then(|ls| ls.overview.as_deref())
                .filter(|s| !s.trim().is_empty())
                .or(s.overview.as_deref().filter(|s| !s.trim().is_empty()))
                .map(str::to_owned);

            seasons.push(SeasonInfo {
                number: match s.season_number {
                    Some(n) => SeasonNumber::from_ordinal(n),
                    _ => SeasonNumber::Specials,
                },
                air_date: opt_date(s.air_date.as_deref())
                    .map(|d| d.to_timestamp_at_midnight_utc())
                    .transpose()?,
                name,
                overview,
                poster: s.poster_path.as_deref().map(ImageKey::tmdb),
            })
        }

        let mut remotes = vec![ShowRemote {
            slug: None,
            remote: Remote::tmdb(id),
        }];

        if let Some(tvdb_id) = details.external_ids.tvdb_id {
            remotes.push(ShowRemote {
                slug: None,
                remote: Remote::tvdb(tvdb_id),
            });
        }

        if let Some(ref imdb_id) = details.external_ids.imdb_id
            && !imdb_id.is_empty()
        {
            remotes.push(ShowRemote {
                slug: None,
                remote: Remote::imdb(imdb_id),
            });
        }

        let posters = to_images(images.posters);
        let backdrops = to_images(images.backdrops);

        let selected_poster = best_image(&posters, poster_path.as_deref().map(ImageKey::tmdb));

        let selected_backdrop =
            best_image(&backdrops, backdrop_path.as_deref().map(ImageKey::tmdb));

        // Prefer the localized title/overview; fall back to the original-language
        // name, then whatever the default language returned.
        let title = localized
            .as_ref()
            .and_then(|l| l.name.as_deref())
            .or(details.name.as_deref())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .or(details.original_name);

        let overview = localized
            .as_ref()
            .and_then(|l| l.overview.as_deref().filter(|s| !s.trim().is_empty()))
            .map(str::to_owned)
            .or(details.overview);

        Ok(ShowInfo {
            title,
            overview,
            original_language,
            first_air_date: opt_date(first_air_date.as_deref())
                .map(|d| d.to_timestamp_at_midnight_utc())
                .transpose()?,
            posters,
            backdrops,
            selected_poster,
            selected_backdrop,
            seasons,
            remotes,
        })
    }

    pub(crate) async fn fetch_season_episodes(
        &self,
        show_id: u32,
        season: api::SeasonNumber,
        language: api::Locale,
    ) -> Result<Vec<EpisodeInfo>> {
        #[derive(Debug, Deserialize)]
        struct EpisodeResponse {
            #[serde(default)]
            episode_number: u32,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            air_date: Option<String>,
            #[serde(default)]
            still_path: Option<String>,
        }

        #[derive(Deserialize)]
        struct SeasonResponse {
            #[serde(default)]
            episodes: Vec<EpisodeResponse>,
        }

        let resp: SeasonResponse = self
            .get_json(
                format!("tv/{show_id}/season/{}", season.ordinal()),
                language,
            )
            .await?;

        let mut updates = Vec::new();

        for e in resp.episodes {
            updates.push(EpisodeInfo {
                season,
                number: e.episode_number,
                name: e.name.filter(|s| !s.is_empty()),
                overview: e.overview,
                aired: opt_date(e.air_date.as_deref())
                    .map(|d| d.to_timestamp_at_midnight_utc())
                    .transpose()?,
                filename: e.still_path.as_deref().map(ImageKey::tmdb),
            });
        }

        Ok(updates)
    }

    pub(crate) async fn fetch_show_translations(&self, id: u32) -> Result<Vec<TranslationRow>> {
        let resp: TmdbTranslationsResponse = self
            .get_json(format!("tv/{id}/translations"), api::Locale::DEFAULT)
            .await?;
        Ok(parse_translations(resp, false))
    }

    pub(crate) async fn fetch_season_translations(
        &self,
        show_id: u32,
        season: api::SeasonNumber,
    ) -> Result<Vec<TranslationRow>> {
        let resp: TmdbTranslationsResponse = self
            .get_json(
                format!("tv/{show_id}/season/{}/translations", season.ordinal()),
                api::Locale::DEFAULT,
            )
            .await?;
        Ok(parse_translations(resp, false))
    }

    pub(crate) async fn fetch_movie_translations(&self, id: u32) -> Result<Vec<TranslationRow>> {
        let resp: TmdbTranslationsResponse = self
            .get_json(format!("movie/{id}/translations"), api::Locale::DEFAULT)
            .await?;
        Ok(parse_translations(resp, true))
    }

    pub(crate) async fn fetch_movie_releases(&self, id: u32) -> Result<Vec<MovieReleaseInfo>> {
        pub fn release_type_from_tmdb(n: u8) -> ReleaseType {
            match n {
                1 => ReleaseType::Premiere,
                2 => ReleaseType::TheatricalLimited,
                3 => ReleaseType::Theatrical,
                4 => ReleaseType::Digital,
                5 => ReleaseType::Physical,
                6 => ReleaseType::Tv,
                _ => ReleaseType::Unknown,
            }
        }

        #[derive(Deserialize)]
        struct Entry {
            #[serde(rename = "type")]
            type_: u8,
            #[serde(default)]
            release_date: Option<String>,
        }

        #[derive(Deserialize)]
        struct CountryBlock {
            iso_3166_1: String,
            #[serde(default)]
            release_dates: Vec<Entry>,
        }

        #[derive(Deserialize)]
        struct Resp {
            #[serde(default)]
            results: Vec<CountryBlock>,
        }

        let d: Resp = self
            .get_json(format!("movie/{id}/release_dates"), api::Locale::DEFAULT)
            .await?;

        let mut out = Vec::new();

        for block in d.results {
            for e in block.release_dates {
                let release_type = release_type_from_tmdb(e.type_);

                let Some(release_date) = parse_release_date(e.release_date.as_deref()) else {
                    continue;
                };

                out.push(MovieReleaseInfo {
                    country: api::Country::from_iso(&block.iso_3166_1).unwrap_or_default(),
                    release_type,
                    release_date,
                });
            }
        }

        Ok(out)
    }

    pub(crate) async fn fetch_movie(&self, id: u32, language: api::Locale) -> Result<MovieInfo> {
        #[derive(Debug, Deserialize, Default)]
        struct ExternalIds {
            #[serde(default)]
            imdb_id: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct Details {
            #[serde(default)]
            poster_path: Option<String>,
            #[serde(default)]
            backdrop_path: Option<String>,
            #[serde(default)]
            original_language: api::Locale,
            #[serde(default)]
            external_ids: ExternalIds,
        }

        let details: Details = self
            .get_json(
                format!("movie/{id}?append_to_response=external_ids"),
                language,
            )
            .await?;

        let effective_language = language.or(details
            .original_language
            .filter(|l| l != api::Locale::EN_US));

        let images: Images = self
            .get_images(format!("movie/{id}/images"), effective_language)
            .await
            .context("Fetching images")?;

        let poster_path = details.poster_path;
        let backdrop_path = details.backdrop_path;
        let original_language = details.original_language;

        let mut remotes = vec![Remote::tmdb(id)];

        if let Some(ref imdb_id) = details.external_ids.imdb_id
            && !imdb_id.is_empty()
        {
            remotes.push(Remote::imdb(imdb_id));
        }

        let posters = to_images(images.posters);
        let backdrops = to_images(images.backdrops);

        let selected_poster = best_image(&posters, poster_path.as_deref().map(ImageKey::tmdb));

        let selected_backdrop =
            best_image(&backdrops, backdrop_path.as_deref().map(ImageKey::tmdb));

        Ok(MovieInfo {
            original_language,
            posters,
            backdrops,
            selected_poster,
            selected_backdrop,
            remotes,
        })
    }
}

pub(crate) struct ShowRemote {
    #[allow(dead_code)]
    pub slug: Option<String>,
    pub remote: Remote,
}

pub(crate) struct ShowInfo {
    pub title: Option<String>,
    pub overview: Option<String>,
    pub original_language: api::Locale,
    pub first_air_date: Option<Timestamp>,
    pub posters: Vec<Image>,
    pub backdrops: Vec<Image>,
    pub selected_poster: Option<ImageKey>,
    pub selected_backdrop: Option<ImageKey>,
    pub seasons: Vec<SeasonInfo>,
    pub remotes: Vec<ShowRemote>,
}

pub(crate) struct SeasonInfo {
    pub number: SeasonNumber,
    pub air_date: Option<Timestamp>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub poster: Option<ImageKey>,
}

pub(crate) struct EpisodeInfo {
    pub season: SeasonNumber,
    pub number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub aired: Option<Timestamp>,
    pub filename: Option<ImageKey>,
}

pub(crate) struct MovieInfo {
    pub original_language: api::Locale,
    pub posters: Vec<Image>,
    pub backdrops: Vec<Image>,
    pub selected_poster: Option<ImageKey>,
    pub selected_backdrop: Option<ImageKey>,
    pub remotes: Vec<Remote>,
}

pub(crate) struct SearchShowResult {
    pub remote: Remote,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub first_air_date: Option<Date>,
    pub poster: Option<ImageKey>,
    pub backdrop: Option<ImageKey>,
}

pub(crate) struct SearchMovieResult {
    pub remote: Remote,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub release_date: Option<Date>,
    pub poster: Option<ImageKey>,
    pub backdrop: Option<ImageKey>,
}

pub(crate) struct MovieReleaseInfo {
    pub country: api::Country,
    pub release_type: ReleaseType,
    pub release_date: Timestamp,
}

fn opt_date(s: Option<&str>) -> Option<Date> {
    s.filter(|s| !s.is_empty()).and_then(|s| s.parse().ok())
}

fn parse_release_date(s: Option<&str>) -> Option<Timestamp> {
    s?.trim().parse().ok()
}

/// Convert TMDB image entries to `Image`s, ordered best-first by a vote-weighted
/// Bayesian rating (see [`weighted_rating`]) so an image with a high average but
/// very few votes can't outrank a well-voted one.
fn to_images(entries: Vec<ImageResponse>) -> Vec<Image> {
    if entries.is_empty() {
        return Vec::new();
    }

    let n = entries.len() as f64;
    let mean_rating = entries.iter().map(|e| e.vote_average).sum::<f64>() / n;
    let mean_votes = entries.iter().map(|e| e.vote_count as f64).sum::<f64>() / n;

    let mut scored: Vec<(f64, Image)> = entries
        .into_iter()
        .map(|e| {
            let score = weighted_rating(&e, mean_rating, mean_votes);
            let image = Image::new_with_dims(ImageSource::Tmdb, &e.file_path, e.width, e.height);
            (score, image)
        })
        .collect();

    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.into_iter().map(|(_, image)| image).collect()
}

fn weighted_rating(img: &ImageResponse, mean_rating: f64, m: f64) -> f64 {
    let v = img.vote_count as f64;

    if v + m == 0.0 {
        return mean_rating;
    }

    (v / (v + m)) * img.vote_average + (m / (v + m)) * mean_rating
}

#[derive(Debug, Deserialize)]
struct ImageResponse {
    file_path: String,
    width: u32,
    height: u32,
    vote_average: f64,
    vote_count: u32,
}

#[derive(Debug, Deserialize)]
struct Images {
    #[serde(default)]
    backdrops: Vec<ImageResponse>,
    #[serde(default)]
    posters: Vec<ImageResponse>,
}

#[derive(Deserialize)]
struct TmdbTranslationEntry {
    iso_639_1: String,
    iso_3166_1: String,
    data: TmdbTranslationData,
}

#[derive(Deserialize, Default)]
struct TmdbTranslationData {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    overview: Option<String>,
}

#[derive(Deserialize)]
struct TmdbTranslationsResponse {
    #[serde(default)]
    translations: Vec<TmdbTranslationEntry>,
}

pub(crate) struct TranslationRow {
    pub locale: api::Locale,
    pub name: Option<String>,
    pub overview: Option<String>,
}

fn locale_from_tmdb(iso_639_1: &str, iso_3166_1: &str) -> Option<api::Locale> {
    let language = api::Language::from_iso(iso_639_1)?;
    if language.is_default() {
        return None;
    }
    let country = api::Country::from_iso(iso_3166_1).unwrap_or_default();
    Some(api::Locale::new(language, country))
}

fn parse_translations(resp: TmdbTranslationsResponse, use_title: bool) -> Vec<TranslationRow> {
    resp.translations
        .into_iter()
        .filter_map(|e| {
            let locale = locale_from_tmdb(&e.iso_639_1, &e.iso_3166_1)?;
            let name = if use_title { e.data.title } else { e.data.name };
            let name = name.filter(|s| !s.trim().is_empty());
            let overview = e.data.overview.filter(|s| !s.trim().is_empty());
            // Skip entries with no actual translated content (TMDB includes
            // placeholder entries for locales where no translation exists).
            if name.is_none() && overview.is_none() {
                return None;
            }
            Some(TranslationRow {
                locale,
                name,
                overview,
            })
        })
        .collect()
}
