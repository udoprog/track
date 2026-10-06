use anyhow::{Context as _, Result};
use musli_web::ws;

use super::WsHandler;

impl WsHandler {
    pub(super) async fn select_image(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SelectImageRequest>()
            .context("Expected a request payload")?;

        let owner = self.db.select_image(req.id).await?;

        match owner {
            api::ImageOwner::Show(show_id) => {
                let show = self
                    .db
                    .show_by_id(Some(self.user.id), show_id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws select image show changed",
                );
            }
            api::ImageOwner::Movie(movie_id) => {
                let movie = self
                    .db
                    .movie_by_id(Some(self.user.id), movie_id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws select image movie changed",
                );
            }
            api::ImageOwner::Season(season_id) => {
                if let Some(show_id) = self.db.show_id_for_season(season_id).await? {
                    let seasons = self.db.seasons(Some(self.user.id), show_id).await?;
                    self.broadcast.emit(
                        incoming.channel(),
                        api::AppEventKind::SeasonsChanged { show_id, seasons },
                        "ws select image season changed",
                    );
                }
            }
        }

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn clear_selected_image(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ClearSelectedImageRequest>()
            .context("Expected a request payload")?;

        self.db.clear_selected_image(req.owner, req.kind).await?;

        match req.owner {
            api::ImageOwner::Show(show_id) => {
                let show = self
                    .db
                    .show_by_id(Some(self.user.id), show_id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws clear selected image show changed",
                );
            }
            api::ImageOwner::Movie(movie_id) => {
                let movie = self
                    .db
                    .movie_by_id(Some(self.user.id), movie_id)
                    .await?
                    .context("Expected movie to exist")?;
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws clear selected image movie changed",
                );
            }
            api::ImageOwner::Season(season_id) => {
                if let Some(show_id) = self.db.show_id_for_season(season_id).await? {
                    let seasons = self.db.seasons(Some(self.user.id), show_id).await?;
                    self.broadcast.emit(
                        incoming.channel(),
                        api::AppEventKind::SeasonsChanged { show_id, seasons },
                        "ws clear selected image season changed",
                    );
                }
            }
        }

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn pick_best_images(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::PickBestImagesRequest>()
            .context("Expected a request payload")?;

        match req.owner {
            api::ImageOwner::Show(show_id) => {
                self.db
                    .pick_best_show_image(show_id, req.kind, true)
                    .await?;

                let show = self
                    .db
                    .show_by_id(Some(self.user.id), show_id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws pick best images show changed",
                );
            }
            api::ImageOwner::Movie(movie_id) => {
                self.db
                    .pick_best_movie_image(movie_id, req.kind, true)
                    .await?;

                let movie = self
                    .db
                    .movie_by_id(Some(self.user.id), movie_id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws pick best images movie changed",
                );
            }
            api::ImageOwner::Season(_) => {}
        }

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn reset_image_selection(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ResetImageSelectionRequest>()
            .context("Expected a request payload")?;

        match req.owner {
            api::ImageOwner::Show(show_id) => {
                self.db
                    .pick_best_show_image(show_id, Some(req.kind), false)
                    .await?;

                let show = self
                    .db
                    .show_by_id(Some(self.user.id), show_id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws reset image selection show changed",
                );
            }
            api::ImageOwner::Movie(movie_id) => {
                self.db
                    .pick_best_movie_image(movie_id, Some(req.kind), false)
                    .await?;

                let movie = self
                    .db
                    .movie_by_id(Some(self.user.id), movie_id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws reset image selection movie changed",
                );
            }
            api::ImageOwner::Season(_) => {}
        }

        outgoing.write(api::Empty);
        Ok(())
    }
}
