use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use api::{Date, Image, ImageSource, RemoteId, SeasonNumber, Timestamp};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, MutexGuard};

const BASE: &str = "https://api4.thetvdb.com/v4/";
const IMAGE_BASE: &str = "https://artworks.thetvdb.com/";
// v4 tokens are valid for ~1 month; refresh well before that.
const EXPIRATION_SECONDS: u64 = 3600 * 24 * 24;

// Series artwork `type` ids (see `GET /artwork/types`, recordType "series").
const ARTWORK_BANNER: u32 = 1;
const ARTWORK_POSTER: u32 = 2;
const ARTWORK_BACKGROUND: u32 = 3;

struct Credentials {
    expires_at: Instant,
    token: String,
}

struct Inner {
    base: reqwest::Url,
    image_base: reqwest::Url,
    http: reqwest::Client,
    api_key: String,
    pin: Option<String>,
    credentials: Mutex<Credentials>,
}

#[derive(Clone)]
pub(crate) struct Client {
    inner: Arc<Inner>,
}

impl Client {
    pub(crate) fn new(http: reqwest::Client, api_key: String, pin: Option<String>) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                base: reqwest::Url::parse(BASE)?,
                image_base: reqwest::Url::parse(IMAGE_BASE)?,
                http,
                api_key,
                pin,
                credentials: Mutex::new(Credentials {
                    expires_at: Instant::now(),
                    token: String::new(),
                }),
            }),
        })
    }

    /// Fetch a v4 artwork image. Image paths are stored host-relative (the host is
    /// stripped on ingest); the public artwork CDN requires no authentication.
    pub(crate) async fn fetch_image(&self, path: &str) -> Result<Option<bytes::Bytes>> {
        let url = self.inner.image_base.join(path.trim_start_matches('/'))?;

        let resp = self.inner.http.get(url).send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        Ok(Some(resp.error_for_status()?.bytes().await?))
    }

    async fn login(&self) -> Result<MutexGuard<'_, Credentials>> {
        #[derive(Serialize)]
        struct Body<'a> {
            apikey: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            pin: Option<&'a str>,
        }

        #[derive(Deserialize)]
        struct Resp {
            data: Data,
        }

        #[derive(Deserialize)]
        struct Data {
            token: String,
        }

        let now = Instant::now();

        let mut creds = self.inner.credentials.lock().await;

        if creds.expires_at > now {
            return Ok(creds);
        }

        let body = serde_json::to_vec(&Body {
            apikey: &self.inner.api_key,
            pin: self.inner.pin.as_deref(),
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
            token: resp.data.token,
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
        struct Row {
            #[serde(default)]
            tvdb_id: Option<String>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            first_air_time: Option<String>,
            #[serde(default)]
            image_url: Option<String>,
            #[serde(default)]
            poster: Option<String>,
            #[serde(default)]
            thumbnail: Option<String>,
        }

        #[derive(Deserialize)]
        struct Resp {
            data: Vec<serde_json::Value>,
        }

        let bytes = self
            .request(Method::GET, "search")
            .await?
            .query(&[("query", query), ("type", "series")])
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let resp: Resp = serde_json::from_slice(&bytes)?;

        let mut out = Vec::new();

        for val in resp.data {
            let row: Row = serde_json::from_value(val)?;

            let Some(id) = row.tvdb_id.as_deref().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };

            let poster = row.poster.or(row.image_url).or(row.thumbnail);

            out.push(SearchSeriesResult {
                remote_id: RemoteId::tvdb(id),
                title: row.name,
                overview: row.overview,
                first_air_date: opt_date(row.first_air_time.as_deref()),
                poster: opt_image(poster.as_deref()),
                banner: None,
            });
        }

        Ok(out)
    }

    pub(crate) async fn fetch_series(&self, id: u32, language: Option<&str>) -> Result<SeriesInfo> {
        let language = language.and_then(tvdb_language);

        #[derive(Deserialize)]
        struct Resp {
            data: Extended,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Extended {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            image: Option<String>,
            #[serde(default)]
            remote_ids: Vec<RemoteIdRow>,
            #[serde(default)]
            artworks: Vec<Artwork>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RemoteIdRow {
            #[serde(default)]
            id: String,
            #[serde(default)]
            source_name: String,
        }

        #[derive(Deserialize)]
        struct Artwork {
            #[serde(default)]
            image: Option<String>,
            #[serde(default)]
            r#type: u32,
            #[serde(default)]
            score: f64,
            #[serde(default)]
            width: u32,
            #[serde(default)]
            height: u32,
        }

        let bytes = self
            .request(Method::GET, format!("series/{id}/extended"))
            .await?
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let resp: Resp = serde_json::from_slice(&bytes)?;
        let v = resp.data;

        let mut title = v.name;
        let mut overview = v.overview;

        // Override title/overview with the configured language's translation.
        if let Some(language) = &language {
            if let Some(tr) = self.fetch_series_translation(id, language).await? {
                if tr.name.as_deref().is_some_and(|s| !s.trim().is_empty()) {
                    title = tr.name;
                }
                if tr.overview.as_deref().is_some_and(|s| !s.trim().is_empty()) {
                    overview = tr.overview;
                }
            }
        }

        let mut remotes = vec![RemoteId::tvdb(id)];

        for remote in &v.remote_ids {
            if remote.source_name.eq_ignore_ascii_case("imdb") && !remote.id.is_empty() {
                remotes.push(RemoteId::imdb(&remote.id));
            }
        }

        // Collect artworks of a given type, ordered best (highest score) first.
        let collect = |kind: u32| -> Vec<(f64, Image)> {
            let mut images: Vec<(f64, Image)> = v
                .artworks
                .iter()
                .filter(|a| a.r#type == kind)
                .filter_map(|a| {
                    let path = image_path(a.image.as_deref()?)?;
                    Some((
                        a.score,
                        Image::new_with_dims(ImageSource::Tvdb, &path, a.width, a.height),
                    ))
                })
                .collect();
            images.sort_by(|a, b| b.0.total_cmp(&a.0));
            images
        };

        let posters = collect(ARTWORK_POSTER);
        let banners = collect(ARTWORK_BANNER);
        let fanart = collect(ARTWORK_BACKGROUND);

        let mut poster: Vec<Image> = posters.iter().map(|(_, i)| i.clone()).collect();
        let banner: Vec<Image> = banners.iter().map(|(_, i)| i.clone()).collect();
        let fanart_imgs: Vec<Image> = fanart.iter().map(|(_, i)| i.clone()).collect();

        // Fall back to the base record image as a poster when no poster artwork exists.
        if poster.is_empty() {
            if let Some(path) = v.image.as_deref().and_then(image_path) {
                poster.push(Image::tvdb(&path));
            }
        }

        let selected_poster = poster.first().cloned();
        let selected_banner = banner.first().cloned();
        let selected_fanart = fanart_imgs.first().cloned();

        Ok(SeriesInfo {
            title,
            overview,
            poster,
            selected_poster,
            banner,
            selected_banner,
            fanart: fanart_imgs,
            selected_fanart,
            remotes,
        })
    }

    async fn fetch_series_translation(
        &self,
        id: u32,
        language: &str,
    ) -> Result<Option<Translation>> {
        #[derive(Deserialize)]
        struct Resp {
            data: Translation,
        }

        let resp = self
            .request(Method::GET, format!("series/{id}/translations/{language}"))
            .await?
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let bytes = resp.error_for_status()?.bytes().await?;
        let resp: Resp = serde_json::from_slice(&bytes)?;
        Ok(Some(resp.data))
    }

    pub(crate) async fn fetch_episodes(
        &self,
        series_id: u32,
        language: Option<&str>,
    ) -> Result<Vec<EpisodeInfo>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Row {
            id: u32,
            #[serde(default)]
            season_number: Option<u32>,
            #[serde(default)]
            number: u32,
            #[serde(default)]
            absolute_number: Option<u32>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            overview: Option<String>,
            #[serde(default)]
            image: Option<String>,
            #[serde(default)]
            aired: Option<String>,
        }

        #[derive(Deserialize)]
        struct Data {
            #[serde(default)]
            episodes: Vec<serde_json::Value>,
        }

        #[derive(Deserialize)]
        struct Links {
            #[serde(default)]
            next: Option<String>,
        }

        #[derive(Deserialize)]
        struct Resp {
            data: Data,
            #[serde(default)]
            links: Option<Links>,
        }

        // Default (aired-order) season type, optionally translated to `language`.
        let language = language.and_then(tvdb_language);
        let path = match &language {
            Some(language) => format!("series/{series_id}/episodes/default/{language}"),
            None => format!("series/{series_id}/episodes/default"),
        };

        let mut output = Vec::new();
        let mut page = 0u32;

        loop {
            let bytes = self
                .request(Method::GET, &path)
                .await?
                .query(&[("page", page.to_string().as_str())])
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;

            let resp: Resp = serde_json::from_slice(&bytes)?;

            for val in resp.data.episodes {
                let row: Row = serde_json::from_value(val)?;

                output.push(EpisodeInfo {
                    season: match row.season_number {
                        Some(n) if n > 0 => SeasonNumber::Number(n),
                        _ => SeasonNumber::Specials,
                    },
                    number: row.number,
                    absolute_number: row.absolute_number,
                    name: row.name.filter(|s| !s.trim().is_empty()),
                    overview: row.overview.filter(|s| !s.trim().is_empty()),
                    aired: opt_date(row.aired.as_deref())
                        .map(|d| d.to_timestamp_at_midnight_utc())
                        .transpose()?,
                    image: opt_image(row.image.as_deref()),
                    remote_id: RemoteId::tvdb(row.id),
                });
            }

            // Pagination: `links.next` is a full URL when there are more pages.
            match resp.links.and_then(|l| l.next) {
                Some(next) if !next.is_empty() => page += 1,
                _ => break,
            }
        }

        Ok(output)
    }
}

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Translation {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    overview: Option<String>,
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
    pub image: Option<(ImageSource, String)>,
    pub remote_id: RemoteId,
}

pub(crate) struct SearchSeriesResult {
    pub remote_id: RemoteId,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub first_air_date: Option<Date>,
    pub poster: Option<(ImageSource, String)>,
    pub banner: Option<(ImageSource, String)>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Map a language code to the 3-letter (ISO 639-3) form the v4 API expects. The
/// app stores ISO 639-1 (2-letter) codes; pass any already-3-letter code through.
/// Returns `None` for unknown codes so the caller falls back to the default
/// language rather than requesting a non-existent translation.
fn tvdb_language(code: &str) -> Option<String> {
    let code = code.trim().to_ascii_lowercase();

    if code.len() == 3 {
        return Some(code);
    }

    iso639::Languages::new()
        .get_by_part1(&code)
        .map(|e| e.id.to_string())
}

fn opt_date(s: Option<&str>) -> Option<Date> {
    s.filter(|s| !s.is_empty()).and_then(|s| s.parse().ok())
}

fn opt_image(s: Option<&str>) -> Option<(ImageSource, String)> {
    image_path(s?).map(|path| (ImageSource::Tvdb, path))
}

/// Strip the scheme and host from an absolute artwork URL, returning the
/// host-relative path. v4 artwork URLs are absolute (e.g.
/// `https://artworks.thetvdb.com/banners/...`); we store the path after the host
/// and re-join the artwork base when fetching. Returns `None` for empty input.
fn image_path(s: &str) -> Option<String> {
    let s = s.trim();

    if s.is_empty() {
        return None;
    }

    let path = match s.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map(|(_, path)| path).unwrap_or(""),
        None => s,
    };

    let path = path.trim_start_matches('/');

    if path.is_empty() {
        None
    } else {
        Some(path.to_owned())
    }
}
