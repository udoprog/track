use anyhow::{Context as _, Result};
use api::{Date, Image, ReleaseType, RemoteId, SeasonNumber, Timestamp};
use serde::Deserialize;
use serde::de::DeserializeOwned;

const BASE: &str = "https://api.themoviedb.org/3/";

#[derive(Clone)]
pub(crate) struct Client {
    base: reqwest::Url,
    http: reqwest::Client,
    api_key: String,
}

impl Client {
    pub(crate) fn new(http: reqwest::Client, api_key: String) -> Result<Self> {
        Ok(Self {
            base: reqwest::Url::parse(BASE)?,
            http,
            api_key,
        })
    }

    async fn get_json<T>(&self, url: impl AsRef<str>, language: Option<&str>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let url = self.base.join(url.as_ref())?;

        let mut req = self
            .http
            .get(url)
            .query(&[("api_key", self.api_key.as_str())]);

        if let Some(language) = language.filter(|l| !l.is_empty()) {
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
            first_air_date: Option<String>,
        }

        #[derive(Deserialize)]
        struct Resp {
            results: Vec<Row>,
        }

        let bytes = self
            .http
            .get(self.base.join("search/tv")?)
            .query(&[("api_key", self.api_key.as_str()), ("query", query)])
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let resp: Resp = serde_json::from_slice(&bytes)?;

        Ok(resp
            .results
            .into_iter()
            .map(|r| SearchSeriesResult {
                remote_id: RemoteId::tmdb(r.id),
                title: r.name.or(r.original_name),
                overview: r.overview.filter(|s| !s.trim().is_empty()),
                first_air_date: opt_date(r.first_air_date.as_deref()),
                poster: opt_image(r.poster_path.as_deref()),
            })
            .collect())
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
            release_date: Option<String>,
        }
        #[derive(Deserialize)]
        struct Resp {
            results: Vec<Row>,
        }

        let bytes = self
            .http
            .get(self.base.join("search/movie")?)
            .query(&[("api_key", self.api_key.as_str()), ("query", query)])
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let resp: Resp = serde_json::from_slice(&bytes)?;

        Ok(resp
            .results
            .into_iter()
            .map(|r| SearchMovieResult {
                remote_id: RemoteId::tmdb(r.id),
                title: r.title.or(r.original_title),
                overview: r.overview.filter(|s| !s.trim().is_empty()),
                release_date: opt_date(r.release_date.as_deref()),
                poster: opt_image(r.poster_path.as_deref()),
            })
            .collect())
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

        let d: Details = self
            .get_json(format!("tv/{id}?append_to_response=external_ids"), language)
            .await?;

        let mut seasons = Vec::with_capacity(d.seasons.len());

        for s in d.seasons {
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

        if let Some(tvdb_id) = d.external_ids.tvdb_id {
            remotes.push(RemoteId::tvdb(tvdb_id));
        }

        if let Some(ref imdb_id) = d.external_ids.imdb_id {
            if !imdb_id.is_empty() {
                remotes.push(RemoteId::imdb(imdb_id));
            }
        }

        Ok(SeriesInfo {
            title: d.name.or(d.original_name),
            overview: d.overview,
            first_air_date: opt_date(d.first_air_date.as_deref())
                .map(|d| d.to_timestamp_at_midnight_utc())
                .transpose()?,
            poster: opt_image(d.poster_path.as_deref()),
            fanart: opt_image(d.backdrop_path.as_deref()),
            seasons,
            remotes,
        })
    }

    pub(crate) async fn fetch_season_episodes(
        &self,
        series_id: u32,
        season_number: u32,
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
            .get_json(format!("tv/{series_id}/season/{season_number}"), language)
            .await?;

        let season = if season_number == 0 {
            SeasonNumber::Specials
        } else {
            SeasonNumber::Number(season_number)
        };

        let mut updates = Vec::new();

        for e in resp.episodes {
            tracing::warn!(?e);

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
        #[derive(Deserialize, Default)]
        struct ExternalIds {
            #[serde(default)]
            imdb_id: Option<String>,
        }
        #[derive(Deserialize)]
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

        let d: Details = self
            .get_json(
                format!("movie/{id}?append_to_response=external_ids"),
                language,
            )
            .await?;

        let mut remotes = vec![RemoteId::tmdb(id)];

        if let Some(ref imdb_id) = d.external_ids.imdb_id {
            if !imdb_id.is_empty() {
                remotes.push(RemoteId::imdb(imdb_id));
            }
        }

        Ok(MovieInfo {
            title: d.title.or(d.original_title),
            overview: d.overview,
            release_date: opt_date(d.release_date.as_deref())
                .map(|d| d.to_timestamp_at_midnight_utc())
                .transpose()?,
            poster: opt_image(d.poster_path.as_deref()),
            fanart: opt_image(d.backdrop_path.as_deref()),
            remotes,
        })
    }
}

// ── Output types ─────────────────────────────────────────────────────────────

pub(crate) struct SeriesInfo {
    pub title: Option<String>,
    pub overview: Option<String>,
    pub first_air_date: Option<Timestamp>,
    pub poster: Option<Image>,
    pub fanart: Option<Image>,
    pub seasons: Vec<SeasonInfo>,
    pub remotes: Vec<RemoteId>,
}

pub(crate) struct SeasonInfo {
    pub number: SeasonNumber,
    pub air_date: Option<Timestamp>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub poster: Option<Image>,
}

pub(crate) struct EpisodeInfo {
    pub season: SeasonNumber,
    pub number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub aired: Option<Timestamp>,
    pub filename: Option<Image>,
    pub remote_id: RemoteId,
}

pub(crate) struct MovieInfo {
    pub title: Option<String>,
    pub overview: Option<String>,
    pub release_date: Option<Timestamp>,
    pub poster: Option<Image>,
    pub fanart: Option<Image>,
    pub remotes: Vec<RemoteId>,
}

pub(crate) struct SearchSeriesResult {
    pub remote_id: RemoteId,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub first_air_date: Option<Date>,
    pub poster: Option<Image>,
}

pub(crate) struct SearchMovieResult {
    pub remote_id: RemoteId,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub release_date: Option<Date>,
    pub poster: Option<Image>,
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

fn opt_image(s: Option<&str>) -> Option<Image> {
    s.filter(|s| !s.is_empty()).map(Image::tmdb)
}
