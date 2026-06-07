use anyhow::Result;
use api::{SeasonNumber, Timestamp};
use serde::Deserialize;

const BASE: &str = "https://api.tvmaze.com";

#[derive(Clone)]
pub(crate) struct Client {
    http: reqwest::Client,
}

impl Client {
    pub(crate) fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    pub(crate) async fn lookup_by_tvdb(&self, tvdb_id: u32) -> Result<Option<u32>> {
        #[derive(Deserialize)]
        struct Show {
            id: u32,
        }

        let resp = self
            .http
            .get(format!("{BASE}/lookup/shows"))
            .query(&[("thetvdb", tvdb_id.to_string())])
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let bytes = resp.error_for_status()?.bytes().await?;
        let show: Show = serde_json::from_slice(&bytes)?;
        Ok(Some(show.id))
    }

    pub(crate) async fn lookup_by_imdb(&self, imdb_id: &str) -> Result<Option<u32>> {
        #[derive(Deserialize)]
        struct Show {
            id: u32,
        }

        let resp = self
            .http
            .get(format!("{BASE}/lookup/shows"))
            .query(&[("imdb", imdb_id)])
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let bytes = resp.error_for_status()?.bytes().await?;
        let show: Show = serde_json::from_slice(&bytes)?;
        Ok(Some(show.id))
    }

    pub(crate) async fn fetch_episodes(&self, id: u32) -> Result<Vec<EpisodeInfo>> {
        #[derive(Deserialize)]
        struct Row {
            #[serde(default)]
            season: Option<u32>,
            #[serde(default)]
            number: Option<u32>,
            #[serde(default)]
            airstamp: Option<String>,
        }

        let bytes = self
            .http
            .get(format!("{BASE}/shows/{id}/episodes"))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let rows: Vec<Row> = serde_json::from_slice(&bytes)?;

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let aired_at = r
                    .airstamp
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .and_then(|s| s.parse::<Timestamp>().ok())?;
                Some(EpisodeInfo {
                    season: match r.season {
                        Some(n) if n > 0 => SeasonNumber::Number(n),
                        _ => SeasonNumber::Specials,
                    },
                    number: r.number.unwrap_or(0),
                    aired_at,
                })
            })
            .collect())
    }
}

pub(crate) struct EpisodeInfo {
    pub season: SeasonNumber,
    pub number: u32,
    pub aired_at: Timestamp,
}
