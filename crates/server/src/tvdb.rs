use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use api::{Date, Image, RemoteId, SeasonNumber, Timestamp};
use reqwest::{Method, header};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, MutexGuard};

const BASE: &str = "https://api.thetvdb.com/";
const EXPIRATION_SECONDS: u64 = 3600 * 24;

struct Credentials {
    expires_at: Instant,
    token: String,
}

struct Inner {
    base: reqwest::Url,
    http: reqwest::Client,
    api_key: String,
    credentials: Mutex<Credentials>,
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
                http,
                api_key,
                credentials: Mutex::new(Credentials {
                    expires_at: Instant::now(),
                    token: String::new(),
                }),
            }),
        })
    }

    async fn login(&self) -> Result<MutexGuard<'_, Credentials>> {
        #[derive(Serialize)]
        struct Body<'a> {
            apikey: &'a str,
        }

        #[derive(Deserialize)]
        struct Resp {
            token: String,
        }

        let now = Instant::now();

        let mut creds = self.inner.credentials.lock().await;

        if creds.expires_at > now {
            return Ok(creds);
        }

        let body = serde_json::to_vec(&Body {
            apikey: &self.inner.api_key,
        })
        .context("serializing login body")?;

        let bytes = self
            .inner
            .http
            .post(self.inner.base.join("login")?)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .context("tvdb login request")?
            .error_for_status()
            .context("tvdb login status")?
            .bytes()
            .await?;

        let resp: Resp = serde_json::from_slice(&bytes).context("tvdb login response")?;

        let expires_at = now
            .checked_add(Duration::from_secs(EXPIRATION_SECONDS))
            .context("calculating credentials expiration time")?;

        *creds = Credentials {
            token: resp.token.clone(),
            expires_at,
        };

        Ok(creds)
    }

    async fn request(
        &self,
        method: Method,
        path: impl AsRef<str>,
    ) -> Result<reqwest::RequestBuilder, anyhow::Error> {
        let token = self.login().await?;

        let req = self
            .inner
            .http
            .request(method, self.inner.base.join(path.as_ref())?)
            .bearer_auth(&token.token);

        Ok(req)
    }

    pub(crate) async fn search_series(&self, query: &str) -> Result<Vec<SearchSeriesResult>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Row {
            id: u32,
            #[serde(default)]
            series_name: String,
            #[serde(default)]
            poster: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            first_aired: Option<String>,
        }

        #[derive(Deserialize)]
        struct Resp {
            data: Vec<serde_json::Value>,
        }

        let bytes = self
            .request(Method::GET, "search/series")
            .await?
            .query(&[("name", query)])
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let resp: Resp = serde_json::from_slice(&bytes)?;

        let mut out = Vec::new();

        for val in resp.data {
            let row: Row = serde_json::from_value(val)?;

            out.push(SearchSeriesResult {
                remote_id: RemoteId::tvdb(row.id),
                title: Some(row.series_name),
                overview: row.overview,
                first_air_date: opt_date(row.first_aired.as_deref()),
                poster: opt_image(row.poster.as_deref()),
            });
        }

        Ok(out)
    }

    pub(crate) async fn fetch_series(&self, id: u32, language: Option<&str>) -> Result<SeriesInfo> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Value {
            #[serde(default)]
            series_name: String,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            poster: Option<String>,
            #[serde(default)]
            banner: Option<String>,
            #[serde(default)]
            fanart: Option<String>,
            #[serde(default)]
            imdb_id: Option<String>,
        }
        #[derive(Deserialize)]
        struct Resp {
            data: Value,
        }

        let mut req = self.request(Method::GET, format!("series/{id}")).await?;

        if let Some(language) = language {
            req = req.header(header::ACCEPT_LANGUAGE, language);
        }

        let bytes = req.send().await?.error_for_status()?.bytes().await?;
        let resp: Resp = serde_json::from_slice(&bytes)?;
        let v = resp.data;

        let mut remotes = vec![RemoteId::tvdb(id)];

        if let Some(ref imdb_id) = v.imdb_id {
            if !imdb_id.is_empty() {
                remotes.push(RemoteId::imdb(imdb_id));
            }
        }

        let selected_poster = v.poster.as_ref().map(|p| Image::tvdb(p));
        let selected_banner = v.banner.as_ref().map(|p| Image::tvdb(p));
        let selected_fanart = v.fanart.as_ref().map(|p| Image::tvdb(p));

        Ok(SeriesInfo {
            title: Some(v.series_name),
            overview: v.overview,
            poster: selected_poster.iter().cloned().collect(),
            selected_poster,
            banner: selected_banner.iter().cloned().collect(),
            selected_banner,
            fanart: selected_fanart.iter().cloned().collect(),
            selected_fanart,
            remotes,
        })
    }

    pub(crate) async fn fetch_episodes(
        &self,
        series_id: u32,
        language: Option<&str>,
    ) -> Result<Vec<EpisodeInfo>> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Row {
            id: u32,
            #[serde(default)]
            absolute_number: Option<u32>,
            #[serde(default)]
            aired_episode_number: u32,
            #[serde(default)]
            aired_season: Option<u32>,
            #[serde(default)]
            episode_name: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            filename: Option<String>,
            #[serde(default)]
            first_aired: Option<String>,
        }

        #[derive(Deserialize)]
        struct Links {
            #[serde(default)]
            next: Option<u32>,
        }

        #[derive(Deserialize)]
        struct Resp {
            data: Vec<serde_json::Value>,
            links: Links,
        }

        let token = self.login().await?.token.clone();

        let mut output = Vec::new();
        let mut page = None::<u32>;

        loop {
            let mut req = self
                .inner
                .http
                .get(
                    self.inner
                        .base
                        .join(&format!("series/{series_id}/episodes"))?,
                )
                .bearer_auth(&token);

            if let Some(language) = language {
                req = req.header(header::ACCEPT_LANGUAGE, language);
            }

            if let Some(p) = page {
                req = req.query(&[("page", p.to_string().as_str())]);
            }

            let bytes = req.send().await?.error_for_status()?.bytes().await?;
            let resp: Resp = serde_json::from_slice(&bytes)?;

            for val in resp.data {
                let row: Row = serde_json::from_value(val)?;

                output.push(EpisodeInfo {
                    season: match row.aired_season {
                        Some(n) if n > 0 => SeasonNumber::Number(n),
                        _ => SeasonNumber::Specials,
                    },
                    number: row.aired_episode_number,
                    absolute_number: row.absolute_number,
                    name: row.episode_name.filter(|s| !s.trim().is_empty()),
                    overview: row.overview.filter(|s| !s.trim().is_empty()),
                    aired: opt_date(row.first_aired.as_deref())
                        .map(|d| d.to_timestamp_at_midnight_utc())
                        .transpose()?,
                    filename: opt_image(row.filename.as_deref()),
                    remote_id: RemoteId::tvdb(row.id),
                });
            }

            match resp.links.next {
                Some(next) => page = Some(next),
                None => break,
            }
        }

        Ok(output)
    }
}

// ── Output types ─────────────────────────────────────────────────────────────

pub(crate) struct SeriesInfo {
    pub title: Option<String>,
    pub overview: Option<String>,
    pub poster: Vec<Image>,
    pub selected_poster: Option<Image>,
    pub banner: Vec<Image>,
    pub selected_banner: Option<Image>,
    pub fanart: Vec<Image>,
    pub selected_fanart: Option<Image>,
    pub remotes: Vec<RemoteId>,
}

pub(crate) struct EpisodeInfo {
    pub season: SeasonNumber,
    pub number: u32,
    pub absolute_number: Option<u32>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub aired: Option<Timestamp>,
    pub filename: Option<Image>,
    pub remote_id: RemoteId,
}

pub(crate) struct SearchSeriesResult {
    pub remote_id: RemoteId,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub first_air_date: Option<Date>,
    pub poster: Option<Image>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn opt_date(s: Option<&str>) -> Option<Date> {
    s.filter(|s| !s.is_empty()).and_then(|s| s.parse().ok())
}

fn opt_image(s: Option<&str>) -> Option<Image> {
    s.filter(|s| !s.is_empty()).map(Image::tvdb)
}
