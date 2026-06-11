use std::sync::Arc;

use anyhow::{Context as _, Result};
use api::{Date, Image, ImageSource, ReleaseType, RemoteId, SeasonNumber, Timestamp};
use reqwest::{Method, RequestBuilder};
use serde::Deserialize;
use serde::de::DeserializeOwned;

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
    async fn get_json<T>(&self, url: impl AsRef<str>, language: Option<&str>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut req = self.request(Method::GET, url.as_ref())?;

        if let Some(language) = language {
            req = req.query(&[("language", language)]);
        }

        let bytes = req
            .send()
            .await
            .context("request failed")?
            .error_for_status()
            .context("bad status")?
            .bytes()
            .await
            .context("reading body")?;

        serde_json::from_slice(&bytes).context("deserializing JSON")
    }

    pub(crate) async fn fetch_image(&self, path: &str) -> Result<Option<bytes::Bytes>> {
        let url = self.inner.image_base.join(path)?;
        let resp = self.inner.http.get(url).send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        Ok(Some(resp.error_for_status()?.bytes().await?))
    }

    pub(crate) async fn search_series(&self, query: &str) -> Result<Vec<SearchSeriesResult>> {
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
        }

        let bytes = self
            .request(Method::GET, "search/tv")
            .context("building request")?
            .query(&[("query", query)])
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let resp: Resp = serde_json::from_slice(&bytes)?;

        let mut out = Vec::with_capacity(resp.results.len());

        for r in resp.results {
            out.push(SearchSeriesResult {
                remote_id: RemoteId::tmdb(r.id),
                title: r.name.or(r.original_name),
                overview: r.overview.filter(|s| !s.trim().is_empty()),
                first_air_date: opt_date(r.first_air_date.as_deref()),
                poster: opt_image(r.poster_path.as_deref()),
                banner: opt_image(r.backdrop_path.as_deref()),
            });
        }

        Ok(out)
    }

    pub(crate) async fn search_movies(&self, query: &str) -> Result<Vec<SearchMovieResult>> {
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
        }

        let bytes = self
            .request(Method::GET, "search/movie")
            .context("building request")?
            .query(&[("query", query)])
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let resp: Resp = serde_json::from_slice(&bytes)?;

        let mut out = Vec::with_capacity(resp.results.len());

        for r in resp.results {
            out.push(SearchMovieResult {
                remote_id: RemoteId::tmdb(r.id),
                title: r.title.or(r.original_title),
                overview: r.overview.filter(|s| !s.trim().is_empty()),
                release_date: opt_date(r.release_date.as_deref()),
                poster: opt_image(r.poster_path.as_deref()),
                banner: opt_image(r.backdrop_path.as_deref()),
            });
        }

        Ok(out)
    }

    pub(crate) async fn fetch_series(&self, id: u32, language: Option<&str>) -> Result<SeriesInfo> {
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
            seasons: Vec<SeasonDetails>,
            #[serde(default)]
            external_ids: ExternalIds,
        }

        let details: Details = self
            .get_json(format!("tv/{id}?append_to_response=external_ids"), language)
            .await?;

        let images: Images = self
            .get_json(format!("tv/{id}/images"), language)
            .await
            .context("fetching images")?;

        let mut seasons = Vec::with_capacity(details.seasons.len());

        for s in details.seasons {
            seasons.push(SeasonInfo {
                number: match s.season_number {
                    Some(n) if n > 0 => SeasonNumber::Number(n),
                    _ => SeasonNumber::Specials,
                },
                air_date: opt_date(s.air_date.as_deref())
                    .map(|d| d.to_timestamp_at_midnight_utc())
                    .transpose()?,
                name: s.name.filter(|s| !s.trim().is_empty()),
                overview: s.overview.filter(|s| !s.trim().is_empty()),
                poster: opt_image(s.poster_path.as_deref()),
            })
        }

        let mut remotes = vec![RemoteId::tmdb(id)];

        if let Some(tvdb_id) = details.external_ids.tvdb_id {
            remotes.push(RemoteId::tvdb(tvdb_id));
        }

        if let Some(ref imdb_id) = details.external_ids.imdb_id {
            if !imdb_id.is_empty() {
                remotes.push(RemoteId::imdb(imdb_id));
            }
        }

        let mut posters = Vec::new();

        for img in images.posters {
            posters.push(Image::new_with_dims(
                ImageSource::Tmdb,
                &img.file_path,
                img.width,
                img.height,
            ));
        }

        let mut backdrops = Vec::new();

        for img in images.backdrops {
            backdrops.push(Image::new_with_dims(
                ImageSource::Tmdb,
                &img.file_path,
                img.width,
                img.height,
            ));
        }

        let selected_poster = opt_image(details.poster_path.as_deref());
        let selected_backdrop = opt_image(details.backdrop_path.as_deref());

        Ok(SeriesInfo {
            title: details.name.or(details.original_name),
            overview: details.overview,
            first_air_date: opt_date(details.first_air_date.as_deref())
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
        series_id: u32,
        season: api::SeasonNumber,
        language: Option<&str>,
    ) -> Result<Vec<EpisodeInfo>> {
        #[derive(Debug, Deserialize)]
        struct EpisodeResponse {
            id: u32,
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
                format!("tv/{series_id}/season/{}", season.to_u32()),
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
                filename: opt_image(e.still_path.as_deref()),
                remote_id: RemoteId::tmdb(e.id),
            });
        }

        Ok(updates)
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
            .get_json(format!("movie/{id}/release_dates"), None)
            .await?;

        let mut out = Vec::new();

        for block in d.results {
            for e in block.release_dates {
                let release_type = release_type_from_tmdb(e.type_);

                let Some(release_date) = parse_release_date(e.release_date.as_deref()) else {
                    continue;
                };

                out.push(MovieReleaseInfo {
                    country: block.iso_3166_1.clone(),
                    release_type,
                    release_date,
                });
            }
        }

        Ok(out)
    }

    pub(crate) async fn fetch_movie(&self, id: u32, language: Option<&str>) -> Result<MovieInfo> {
        #[derive(Debug, Deserialize, Default)]
        struct ExternalIds {
            #[serde(default)]
            imdb_id: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct Details {
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            original_title: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            poster_path: Option<String>,
            #[serde(default)]
            backdrop_path: Option<String>,
            #[serde(default)]
            release_date: Option<String>,
            #[serde(default)]
            external_ids: ExternalIds,
        }

        let details: Details = self
            .get_json(
                format!("movie/{id}?append_to_response=external_ids"),
                language,
            )
            .await?;

        let images: Images = self
            .get_json(format!("movie/{id}/images"), language)
            .await
            .context("fetching images")?;

        let mut remotes = vec![RemoteId::tmdb(id)];

        if let Some(ref imdb_id) = details.external_ids.imdb_id {
            if !imdb_id.is_empty() {
                remotes.push(RemoteId::imdb(imdb_id));
            }
        }

        let mut posters = Vec::new();

        for img in images.posters {
            posters.push(Image::new_with_dims(
                ImageSource::Tmdb,
                &img.file_path,
                img.width,
                img.height,
            ));
        }

        let mut backdrops = Vec::new();

        for img in images.backdrops {
            backdrops.push(Image::new_with_dims(
                ImageSource::Tmdb,
                &img.file_path,
                img.width,
                img.height,
            ));
        }

        let selected_poster = opt_image(details.poster_path.as_deref());
        let selected_backdrop = opt_image(details.backdrop_path.as_deref());

        Ok(MovieInfo {
            title: details.title.or(details.original_title),
            overview: details.overview,
            release_date: opt_date(details.release_date.as_deref())
                .map(|d| d.to_timestamp_at_midnight_utc())
                .transpose()?,
            posters,
            backdrops,
            selected_poster,
            selected_backdrop,
            remotes,
        })
    }
}

// ── Output types ─────────────────────────────────────────────────────────────

pub(crate) struct SeriesInfo {
    pub title: Option<String>,
    pub overview: Option<String>,
    pub first_air_date: Option<Timestamp>,
    pub posters: Vec<Image>,
    pub backdrops: Vec<Image>,
    pub selected_poster: Option<(ImageSource, String)>,
    pub selected_backdrop: Option<(ImageSource, String)>,
    pub seasons: Vec<SeasonInfo>,
    pub remotes: Vec<RemoteId>,
}

pub(crate) struct SeasonInfo {
    pub number: SeasonNumber,
    pub air_date: Option<Timestamp>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub poster: Option<(ImageSource, String)>,
}

pub(crate) struct EpisodeInfo {
    pub season: SeasonNumber,
    pub number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub aired: Option<Timestamp>,
    pub filename: Option<(ImageSource, String)>,
    pub remote_id: RemoteId,
}

pub(crate) struct MovieInfo {
    pub title: Option<String>,
    pub overview: Option<String>,
    pub release_date: Option<Timestamp>,
    pub posters: Vec<Image>,
    pub backdrops: Vec<Image>,
    pub selected_poster: Option<(ImageSource, String)>,
    pub selected_backdrop: Option<(ImageSource, String)>,
    pub remotes: Vec<RemoteId>,
}

pub(crate) struct SearchSeriesResult {
    pub remote_id: RemoteId,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub first_air_date: Option<Date>,
    pub poster: Option<(ImageSource, String)>,
    pub banner: Option<(ImageSource, String)>,
}

pub(crate) struct SearchMovieResult {
    pub remote_id: RemoteId,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub release_date: Option<Date>,
    pub poster: Option<(ImageSource, String)>,
    pub banner: Option<(ImageSource, String)>,
}

pub(crate) struct MovieReleaseInfo {
    pub country: String,
    pub release_type: ReleaseType,
    pub release_date: Timestamp,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn opt_date(s: Option<&str>) -> Option<Date> {
    s.filter(|s| !s.is_empty()).and_then(|s| s.parse().ok())
}

fn parse_release_date(s: Option<&str>) -> Option<Timestamp> {
    s?.trim().parse().ok()
}

fn opt_image(s: Option<&str>) -> Option<(ImageSource, String)> {
    s.filter(|s| !s.is_empty())
        .map(|s| (ImageSource::Tmdb, s.trim_start_matches('/').to_string()))
}

#[derive(Debug, Deserialize)]
struct ImageResponse {
    file_path: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
struct Images {
    #[serde(default)]
    backdrops: Vec<ImageResponse>,
    #[serde(default)]
    posters: Vec<ImageResponse>,
}
