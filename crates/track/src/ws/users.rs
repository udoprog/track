use anyhow::{Context as _, Result};
use musli_web::ws;
use tokio::task::spawn_blocking;

use super::{Refused, WsHandler, parse_email, parse_login, role_from_api};
use crate::db::users::UserRecord;
use crate::identity::Revoke;

impl WsHandler {
    pub(super) async fn list_users(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        incoming
            .read::<api::ListUsersRequest>()
            .context("Expected a request payload")?;

        let users = self.db.list_users().await?;
        let users = users.iter().map(UserRecord::to_api).collect();

        let login_links = self
            .db
            .pending_login_links(api::Timestamp::now())
            .await?
            .into_iter()
            .map(|link| api::LoginLink {
                user_id: link.user_id,
                expires_at: link.expires_at,
            })
            .collect();

        outgoing.write(api::ListUsersResponse { users, login_links });
        Ok(())
    }

    pub(super) async fn create_user(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::CreateUserRequest>()
            .context("Expected a request payload")?;

        let login = parse_login(&req.login)?;
        let email = parse_email(req.email.as_deref());

        let user = self
            .db
            .create_user(
                &login,
                email.as_deref(),
                role_from_api(req.role),
                api::Timestamp::now(),
            )
            .await?
            .map_err(Refused::from)?;

        outgoing.write(api::UserResponse {
            user: user.to_api(),
        });
        Ok(())
    }

    pub(super) async fn set_user_role(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetUserRoleRequest>()
            .context("Expected a request payload")?;

        self.not_self(req.user_id)?;

        let user = self
            .db
            .set_user_role(req.user_id, role_from_api(req.role))
            .await?
            .ok_or(Refused::NoSuchUser)?;

        outgoing.write(api::UserResponse {
            user: user.to_api(),
        });
        Ok(())
    }

    pub(super) async fn delete_user(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::DeleteUserRequest>()
            .context("Expected a request payload")?;

        self.not_self(req.user_id)?;
        self.db.delete_user(req.user_id).await?;
        self.auth.revoke(Revoke::User(req.user_id));
        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn generate_login_token(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::GenerateLoginTokenRequest>()
            .context("Expected a request payload")?;

        self.db
            .user_by_id(req.user_id)
            .await?
            .ok_or(Refused::NoSuchUser)?;

        let token = auth::new_login_token();
        let expires_at =
            api::Timestamp::from_jiff(auth::login_token_expiry(api::Timestamp::now().into_jiff()));

        self.db
            .create_login_token(&token, req.user_id, expires_at)
            .await?;

        outgoing.write(api::GenerateLoginTokenResponse { token, expires_at });
        Ok(())
    }

    pub(super) async fn revoke_login_token(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::RevokeLoginTokenRequest>()
            .context("Expected a request payload")?;

        self.db.revoke_login_tokens(req.user_id).await?;
        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn revoke_user_access(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::RevokeUserAccessRequest>()
            .context("Expected a request payload")?;

        self.not_self(req.user_id)?;
        self.db.delete_user_sessions(req.user_id).await?;
        self.auth.revoke(Revoke::User(req.user_id));
        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_login(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetLoginRequest>()
            .context("Expected a request payload")?;

        let login = parse_login(&req.login)?;

        let user = self
            .db
            .set_user_login(self.user.id, &login)
            .await?
            .map_err(Refused::from)?
            .ok_or(Refused::NoSuchUser)?;

        outgoing.write(api::UserResponse {
            user: user.to_api(),
        });
        Ok(())
    }

    pub(super) async fn set_email(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetEmailRequest>()
            .context("Expected a request payload")?;

        let email = parse_email(req.email.as_deref());

        let user = self
            .db
            .set_user_email(self.user.id, email.as_deref())
            .await?
            .map_err(Refused::from)?
            .ok_or(Refused::NoSuchUser)?;

        outgoing.write(api::UserResponse {
            user: user.to_api(),
        });
        Ok(())
    }

    pub(super) async fn set_password(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetPasswordRequest>()
            .context("Expected a request payload")?;

        let user = self.current_user().await?;

        if let Some(hash) = user.password_hash.clone() {
            let old_password = req.old_password;

            let verified =
                spawn_blocking(move || auth::verify_password(&old_password, &hash)).await?;

            if !verified {
                return Err(Refused::WrongPassword.into());
            }
        }

        if let Some(message) = auth::validate_password(&req.new_password) {
            return Err(Refused::WeakPassword(message).into());
        }

        let new_password = req.new_password;
        let hash = spawn_blocking(move || auth::hash_password(&new_password)).await??;
        let keep = self.user.session.as_deref();
        self.db.set_user_password_hash(user.id, &hash, keep).await?;
        self.auth.revoke(Revoke::OtherSessions {
            user: user.id,
            keep: keep.map(str::to_owned),
        });
        outgoing.write(api::Empty);
        Ok(())
    }
}
