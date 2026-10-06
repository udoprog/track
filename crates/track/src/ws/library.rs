use anyhow::{Context as _, Result};
use musli_web::ws;

use super::WsHandler;
use crate::remote::interleave;

impl WsHandler {
    pub(super) async fn list_media(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        incoming
            .read::<api::ListMediaRequest>()
            .context("Expected a request payload")?;
        let items = self
            .db
            .media_items(self.user.id)
            .await
            .context("Loading media")?;
        outgoing.write(api::ListMediaResponse { items });
        Ok(())
    }

    pub(super) async fn get_translations(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::GetTranslationsRequest>()
            .context("Expected a request payload")?;

        let translations = match req.target {
            api::TranslationTarget::Show(id) => self.db.show_translations(id).await?,
            api::TranslationTarget::Season(id) => self.db.season_translations(id).await?,
            api::TranslationTarget::Episode(id) => self.db.episode_translations(id).await?,
            api::TranslationTarget::Movie(id) => self.db.movie_translations(id).await?,
        };

        outgoing.write(api::GetTranslationsResponse { translations });
        Ok(())
    }

    pub(super) async fn list_credits(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ListCreditsRequest>()
            .context("Expected a request payload")?;

        let credits = match req.owner {
            api::CreditOwner::Show(id) => self.db.list_show_credits(self.user.id, id).await?,
            api::CreditOwner::Movie(id) => self.db.list_movie_credits(self.user.id, id).await?,
        };

        outgoing.write(api::ListCreditsResponse { credits });
        Ok(())
    }

    pub(super) async fn search(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SearchRequest>()
            .context("Expected a request payload")?;

        let mut shows: Vec<api::SearchShow> = Vec::new();
        let mut movies: Vec<api::SearchMovie> = Vec::new();

        // Search spans both kinds; query only the selected ones and sum
        // their totals so pagination accounts for every source.
        let mut total = 0;

        if req.shows {
            let (results, count) = self.remote.search_show(&req.query, req.page).await?;

            total += count;

            for r in results {
                let already_tracked = self
                    .db
                    .shows_by_remote_id(Some(self.user.id), &r.remote)
                    .await?
                    .filter(|s| s.tracked)
                    .map(|s| s.id);

                shows.push(api::SearchShow {
                    already_tracked,
                    ..r
                });
            }
        }

        if req.movies {
            let (results, count) = self.remote.search_movies(&req.query, req.page).await?;

            total += count;

            for r in results {
                let already_tracked = self
                    .db
                    .movie_by_remote_id(Some(self.user.id), &r.remote)
                    .await?
                    .filter(|m| m.tracked)
                    .map(|m| m.id);

                movies.push(api::SearchMovie {
                    already_tracked,
                    ..r
                });
            }
        }

        // Interleave the two kinds round-robin so results are mixed
        // through the list, while preserving each source's own order.
        let results = interleave(
            shows.into_iter().map(api::SearchResult::Show).collect(),
            movies.into_iter().map(api::SearchResult::Movie).collect(),
        );

        outgoing.write(api::SearchResponse { results, total });
        Ok(())
    }
}
