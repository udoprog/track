use anyhow::Result;
use api::ShowId;
use sqll::{Row, Statements, TypedStatement};
use tokio::task::spawn_blocking;

use super::{Database, InnerRead, InnerWrite};
use crate::xem::{self, Name, Numbering};

#[derive(Statements)]
#[sql(read_only)]
pub(super) struct Read {
    #[sql = "SELECT system, part, season, episode, absolute FROM xem_episodes"]
    #[sql = "WHERE show_id = ? AND entry = ("]
    #[sql = "  SELECT entry FROM xem_episodes WHERE show_id = ? AND system = ? AND season = ? AND episode = ?"]
    #[sql = "  ORDER BY entry LIMIT 1"]
    #[sql = ") ORDER BY system, part"]
    entry: TypedStatement<(ShowId, ShowId, String, u32, u32), Numbering>,
    #[sql = "SELECT season, language, name FROM xem_names WHERE show_id = ?"]
    #[sql = "ORDER BY season IS NOT NULL, season, language, name"]
    names: TypedStatement<(ShowId,), Name>,
    #[sql = "SELECT season, episode FROM episodes WHERE show_id = ? ORDER BY season, episode"]
    episode_codes: TypedStatement<(ShowId,), Code>,
    #[sql = "SELECT system, season, episode, MIN(entry) AS entry FROM xem_episodes"]
    #[sql = "WHERE show_id = ? AND part = 0"]
    #[sql = "GROUP BY system, season, episode ORDER BY system, season, episode"]
    system_codes: TypedStatement<(ShowId,), SystemCode>,
    #[sql = "SELECT DISTINCT o.season AS season FROM xem_episodes t"]
    #[sql = "JOIN xem_episodes o ON o.show_id = t.show_id AND o.entry = t.entry"]
    #[sql = "WHERE t.show_id = ? AND t.system = ? AND t.season = ? AND o.system = ? AND o.part = 0"]
    #[sql = "ORDER BY o.season"]
    origin_seasons: TypedStatement<(ShowId, String, u32, String), Season>,
}

#[derive(Row)]
struct Season {
    season: u32,
}

#[derive(Row)]
struct Code {
    season: u32,
    episode: u32,
}

#[derive(Row)]
struct SystemCode {
    system: String,
    season: u32,
    episode: u32,
    entry: u32,
}

#[derive(Statements)]
pub(super) struct Write {
    #[sql = "DELETE FROM xem_episodes WHERE show_id = ?"]
    clear_episodes: TypedStatement<(ShowId,), ()>,
    #[sql = "INSERT INTO xem_episodes (show_id, entry, system, part, season, episode, absolute) VALUES (?, ?, ?, ?, ?, ?, ?)"]
    insert_episode: TypedStatement<(ShowId, u32, String, u32, u32, u32, Option<u32>), ()>,
    #[sql = "DELETE FROM xem_names WHERE show_id = ?"]
    clear_names: TypedStatement<(ShowId,), ()>,
    #[sql = "INSERT OR IGNORE INTO xem_names (show_id, season, language, name) VALUES (?, ?, ?, ?)"]
    insert_name: TypedStatement<(ShowId, Option<u32>, Option<String>, String), ()>,
}

impl InnerWrite {
    /// Replace the show's XEM map with `entries`, one list of addresses per
    /// map/all entry in its order.
    pub(crate) fn replace_xem_episodes(
        &mut self,
        show_id: ShowId,
        entries: &[Vec<Numbering>],
    ) -> Result<()> {
        self.xem_write.clear_episodes.execute((show_id,))?;

        for (index, entry) in entries.iter().enumerate() {
            for n in entry {
                self.xem_write.insert_episode.execute((
                    show_id,
                    index as u32,
                    n.system.as_str(),
                    n.part,
                    n.season,
                    n.episode,
                    n.absolute,
                ))?;
            }
        }

        Ok(())
    }

    pub(crate) fn replace_xem_names(&mut self, show_id: ShowId, names: &[Name]) -> Result<()> {
        self.xem_write.clear_names.execute((show_id,))?;

        for n in names {
            self.xem_write.insert_name.execute((
                show_id,
                n.season,
                n.language.as_deref(),
                n.name.as_str(),
            ))?;
        }

        Ok(())
    }
}

impl InnerRead {
    fn xem_names(&mut self, show_id: ShowId) -> Result<Vec<Name>> {
        let mut out = Vec::new();
        let mut stmt = self.xem.names.bind((show_id,))?;

        while let Some(row) = stmt.next()? {
            out.push(row);
        }

        Ok(out)
    }

    /// XEM's names for the whole show, leaving out its own titles.
    pub(super) fn xem_show_names(&mut self, show: &api::Show) -> Result<Vec<api::AltName>> {
        let names = self.xem_names(show.id)?;
        let titles = show.strings.texts(api::StringKind::Title);
        Ok(xem::names_for(&names, &[None], titles))
    }
}

impl Database {
    #[cfg(test)]
    pub(crate) async fn xem_names(&self, show_id: ShowId) -> Result<Vec<Name>> {
        let mut s = self.inner.clone().shared().await?;
        spawn_blocking(move || s.xem_names(show_id)).await?
    }

    /// For each target, every system's address of the XEM entry holding it;
    /// empty for no target or when XEM maps no such episode.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn xem_entries(
        &self,
        show_id: ShowId,
        targets: Vec<Option<api::NumberingTarget>>,
    ) -> Result<Vec<Vec<Numbering>>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::with_capacity(targets.len());

            for target in targets {
                let mut entry = Vec::new();

                if let Some(t) = target {
                    let mut stmt = s
                        .xem
                        .entry
                        .bind((show_id, show_id, t.system, t.season, t.episode))?;

                    while let Some(row) = stmt.next()? {
                        entry.push(row);
                    }
                }

                out.push(entry);
            }

            Ok(out)
        });

        result.await?
    }

    /// The XEM names of each of `seasons`' target seasons, for those that have
    /// any. XEM numbers season names by `origin`, so a target in another
    /// system is read through the origin seasons its episodes are in.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn xem_season_names(
        &self,
        show_id: ShowId,
        origin: String,
        seasons: Vec<Vec<api::LinkedSeason>>,
    ) -> Result<Vec<Vec<api::SeasonAltNames>>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let names = s.xem_names(show_id)?;
            let mut out = Vec::with_capacity(seasons.len());

            for targets in seasons {
                let mut season_names = Vec::new();

                for target in targets {
                    let origin_seasons = if target.system == origin {
                        vec![Some(target.season)]
                    } else {
                        let mut out = Vec::new();
                        let mut stmt = s.xem.origin_seasons.bind((
                            show_id,
                            target.system.as_str(),
                            target.season,
                            origin.as_str(),
                        ))?;

                        while let Some(row) = stmt.next()? {
                            out.push(Some(row.season));
                        }

                        out
                    };

                    let names = xem::names_for(&names, &origin_seasons, []);

                    if !names.is_empty() {
                        season_names.push(api::SeasonAltNames { target, names });
                    }
                }

                out.push(season_names);
            }

            Ok(out)
        });

        result.await?
    }

    /// The show's episode codes, and XEM's codes for it by system, each in
    /// episode order.    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn numbering_codes(
        &self,
        show_id: ShowId,
    ) -> Result<(Vec<(u32, u32)>, Vec<api::XemSystemEpisodes>)> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut episodes = Vec::new();

            {
                let mut stmt = s.xem.episode_codes.bind((show_id,))?;

                while let Some(c) = stmt.next()? {
                    episodes.push((c.season, c.episode));
                }
            }

            let mut systems = Vec::<api::XemSystemEpisodes>::new();
            let mut stmt = s.xem.system_codes.bind((show_id,))?;

            while let Some(c) = stmt.next()? {
                match systems.last_mut() {
                    Some(last) if last.system == c.system => {
                        last.episodes.push((c.season, c.episode));
                        last.entries.push(c.entry);
                    }
                    _ => systems.push(api::XemSystemEpisodes {
                        system: c.system,
                        episodes: vec![(c.season, c.episode)],
                        entries: vec![c.entry],
                    }),
                }
            }

            Ok((episodes, systems))
        });

        result.await?
    }
}
