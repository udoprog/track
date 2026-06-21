use anyhow::Result;
use api::{Country, SeasonNumber, Timestamp};
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

    /// Fetch the show's primary network (or web channel) name and country code,
    /// applied to all of its TVmaze episode air dates.
    pub(crate) async fn fetch_show_network(&self, id: u32) -> Result<ShowNetwork> {
        #[derive(Deserialize)]
        struct Country {
            code: Option<String>,
        }

        #[derive(Deserialize)]
        struct Network {
            name: Option<String>,
            country: Option<Country>,
        }

        #[derive(Deserialize)]
        struct Show {
            network: Option<Network>,
            #[serde(rename = "webChannel")]
            web_channel: Option<Network>,
        }

        let bytes = self
            .http
            .get(format!("{BASE}/shows/{id}"))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let show: Show = serde_json::from_slice(&bytes)?;

        let net = show.network.or(show.web_channel);

        Ok(match net {
            Some(n) => ShowNetwork {
                network: n.name.unwrap_or_default(),
                country: n
                    .country
                    .and_then(|c| api::Country::from_iso_3166_1(c.code.as_deref()?))
                    .unwrap_or_default(),
            },
            None => ShowNetwork::default(),
        })
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
                        Some(n) => api::SeasonNumber::from_ordinal(n),
                        _ => api::SeasonNumber::Specials,
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

/// The network / country a show airs on, applied to its TVmaze air dates.
#[derive(Default)]
pub(crate) struct ShowNetwork {
    pub network: String,
    pub country: Country,
}
