use anyhow::Result;
use api::ShowId;
use sqll::{Statements, TypedStatement};
use tokio::task::spawn_blocking;

use super::{Database, InnerWrite};
use crate::xem::{Name, Numbering};

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

// The lookups the numbering display reads; nothing calls them yet.
#[allow(dead_code)]
impl Database {
    /// The XEM entry holding `system`'s (`season`, `episode`), as every
    /// system's address of it; empty when XEM maps no such episode.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn xem_entry(
        &self,
        show_id: ShowId,
        system: &str,
        season: u32,
        episode: u32,
    ) -> Result<Vec<Numbering>> {
        let mut s = self.inner.clone().shared().await?;
        let system = system.to_owned();

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s
                .xem
                .entry
                .bind((show_id, show_id, system, season, episode))?;

            while let Some(row) = stmt.next()? {
                out.push(row);
            }

            Ok(out)
        });

        result.await?
    }

    /// The show's XEM names, the show's own (season `None`) first.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn xem_names(&self, show_id: ShowId) -> Result<Vec<Name>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.xem.names.bind((show_id,))?;

            while let Some(row) = stmt.next()? {
                out.push(row);
            }

            Ok(out)
        });

        result.await?
    }
}
