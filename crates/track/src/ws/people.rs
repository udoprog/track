use anyhow::{Context as _, Result};
use musli_web::ws;

use super::WsHandler;

impl WsHandler {
    pub(super) async fn list_persons(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        incoming
            .read::<api::ListPersonsRequest>()
            .context("Expected a request payload")?;
        let persons = self.db.list_persons(self.user.id).await?;
        outgoing.write(api::ListPersonsResponse { persons });
        Ok(())
    }

    pub(super) async fn get_person(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::GetPersonRequest>()
            .context("Expected a request payload")?;
        let person = self.db.person_by_id(Some(self.user.id), req.id).await?;
        outgoing.write(person);
        Ok(())
    }

    pub(super) async fn list_person_credits(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ListPersonCreditsRequest>()
            .context("Expected a request payload")?;
        let credits = self.db.list_person_credits(self.user.id, req.id).await?;
        outgoing.write(api::ListPersonCreditsResponse { credits });
        Ok(())
    }

    pub(super) async fn delete_person(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::DeletePersonRequest>()
            .context("Expected a request payload")?;

        self.db.delete_person(req.id).await?;

        // The person is gone; a PersonChanged lets open detail pages resolve
        // to "missing" and the people list drop it.
        self.broadcast.emit(
            incoming.channel(),
            api::AppEventKind::PersonChanged { person_id: req.id },
            "ws delete person",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn sync_person(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SyncPersonRequest>()
            .context("Expected a request payload")?;

        let person = self
            .db
            .person_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected person to exist")?;

        self.enqueue_person_sync(person.id, person.name.title().map(str::to_owned), true)
            .await;

        outgoing.write(api::Empty);
        Ok(())
    }
}
