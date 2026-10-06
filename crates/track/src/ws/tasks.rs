use anyhow::{Context as _, Result};
use musli_web::ws;

use super::WsHandler;

impl WsHandler {
    pub(super) async fn sync_all(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let _req = incoming
            .read::<api::SyncAllRequest>()
            .context("Expected a request payload")?;

        let shows = self.db.shows(None).await?;

        for s in shows {
            self.enqueue_show_sync(s.id, s.strings.title().map(str::to_owned), false)
                .await;
        }

        let movies = self.db.movies(None).await?;

        for m in movies {
            self.enqueue_movie_sync(m.id, m.strings.title().map(str::to_owned), false)
                .await;
        }

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn list_tasks(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let _req = incoming
            .read::<api::ListTasksRequest>()
            .context("Expected a request payload")?;

        let tasks = self.queue.list().await;

        outgoing.write(tasks);
        Ok(())
    }

    pub(super) async fn remove_task(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::RemoveTaskRequest>()
            .context("Expected a request payload")?;

        self.queue.remove(req.id, &self.broadcast).await;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn bump_task(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::BumpTaskRequest>()
            .context("Expected a request payload")?;

        self.queue.bump(req.id, &self.broadcast).await;

        outgoing.write(api::Empty);
        Ok(())
    }
}
