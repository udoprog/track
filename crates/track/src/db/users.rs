use anyhow::{Result, anyhow};
use api::{Timestamp, UserId};
use auth::UserRole;
use sqll::{Row, Statements, TypedStatement};
use tokio::task::spawn_blocking;

use super::Database;

#[derive(Debug, Clone)]
pub(crate) struct UserRecord {
    pub(crate) id: UserId,
    pub(crate) login: String,
    pub(crate) email: Option<String>,
    pub(crate) role: UserRole,
    pub(crate) password_hash: Option<String>,
    pub(crate) created_at: Timestamp,
}

impl UserRecord {
    pub(crate) fn to_api(&self) -> api::User {
        api::User {
            id: self.id,
            login: self.login.clone(),
            email: self.email.clone(),
            role: match self.role {
                UserRole::Admin => api::UserRole::Admin,
                UserRole::Regular => api::UserRole::Regular,
            },
            has_password: self.password_hash.is_some(),
            created_at: self.created_at,
        }
    }
}

#[derive(Row)]
struct UserRow {
    id: UserId,
    login: String,
    email: Option<String>,
    role: String,
    password_hash: Option<String>,
    created_at: Timestamp,
}

impl UserRow {
    fn into_record(self) -> Result<UserRecord> {
        let role = self
            .role
            .parse()
            .map_err(|_| anyhow!("User {} has unknown role {:?}", self.id, self.role))?;

        Ok(UserRecord {
            id: self.id,
            login: self.login,
            email: self.email,
            role,
            password_hash: self.password_hash,
            created_at: self.created_at,
        })
    }
}

/// Why a login or email could not be taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Conflict {
    Login,
    Email,
}

/// Why a login token could not be redeemed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenError {
    NotFound,
    /// Used or expired.
    Gone,
}

#[derive(Row)]
struct LoginTokenRow {
    user_id: UserId,
    expires_at: Timestamp,
    used_at: Option<Timestamp>,
}

impl LoginTokenRow {
    fn check(&self, now: Timestamp) -> Result<(), TokenError> {
        if self.used_at.is_some() || auth::is_expired(self.expires_at.into_jiff(), now.into_jiff())
        {
            return Err(TokenError::Gone);
        }

        Ok(())
    }
}

#[derive(Row)]
pub(crate) struct PendingLoginLink {
    pub(crate) user_id: UserId,
    pub(crate) expires_at: Timestamp,
}

#[derive(Statements)]
#[sql(read_only)]
pub(super) struct Read {
    #[sql = "SELECT id, login, email, role, password_hash, created_at FROM users ORDER BY id"]
    list: TypedStatement<(), UserRow>,
    #[sql = "SELECT id, login, email, role, password_hash, created_at FROM users WHERE id = ?"]
    by_id: TypedStatement<(UserId,), UserRow>,
    #[sql = "SELECT id, login, email, role, password_hash, created_at FROM users WHERE login = ?"]
    by_login: TypedStatement<(String,), UserRow>,
    #[sql = "SELECT id, login, email, role, password_hash, created_at FROM users WHERE email = ?"]
    by_email: TypedStatement<(String,), UserRow>,
    #[sql = "SELECT u.id, u.login, u.email, u.role, u.password_hash, u.created_at"]
    #[sql = "FROM sessions s JOIN users u ON u.id = s.user_id"]
    #[sql = "WHERE s.id = ? AND s.expires_at > ?"]
    by_session: TypedStatement<(String, Timestamp), UserRow>,
    #[sql = "SELECT user_id, expires_at, used_at FROM login_tokens WHERE id = ?"]
    login_token: TypedStatement<(String,), LoginTokenRow>,
    // Root, or the first administrator if root was renamed.
    #[sql = "SELECT id FROM users WHERE role = 'admin' ORDER BY login <> 'root', id LIMIT 1"]
    default_owner: TypedStatement<(), UserId>,
    #[sql = "SELECT user_id, MAX(expires_at) AS expires_at FROM login_tokens"]
    #[sql = "WHERE used_at IS NULL AND expires_at > ? GROUP BY user_id"]
    pending_login_links: TypedStatement<(Timestamp,), PendingLoginLink>,
}

impl Read {
    fn by_id(&mut self, id: UserId) -> Result<Option<UserRecord>> {
        self.by_id
            .bind((id,))?
            .first()?
            .map(UserRow::into_record)
            .transpose()
    }

    fn by_login(&mut self, login: &str) -> Result<Option<UserRecord>> {
        self.by_login
            .bind((login,))?
            .first()?
            .map(UserRow::into_record)
            .transpose()
    }

    fn by_email(&mut self, email: &str) -> Result<Option<UserRecord>> {
        self.by_email
            .bind((email,))?
            .first()?
            .map(UserRow::into_record)
            .transpose()
    }

    /// Whether `login` or `email` belong to a user other than `id`.
    fn conflict(
        &mut self,
        id: Option<UserId>,
        login: Option<&str>,
        email: Option<&str>,
    ) -> Result<Option<Conflict>> {
        if let Some(login) = login
            && let Some(user) = self.by_login(login)?
            && Some(user.id) != id
        {
            return Ok(Some(Conflict::Login));
        }

        if let Some(email) = email
            && let Some(user) = self.by_email(email)?
            && Some(user.id) != id
        {
            return Ok(Some(Conflict::Email));
        }

        Ok(None)
    }
}

#[derive(Statements)]
pub(super) struct Write {
    #[sql = "INSERT INTO users (login, email, role, created_at) VALUES (?, ?, ?, ?) RETURNING id"]
    insert: TypedStatement<(String, Option<String>, String, Timestamp), UserId>,
    #[sql = "DELETE FROM users WHERE id = ?"]
    delete: TypedStatement<(UserId,), ()>,
    #[sql = "UPDATE users SET role = ? WHERE id = ?"]
    set_role: TypedStatement<(String, UserId), ()>,
    #[sql = "UPDATE users SET login = ? WHERE id = ?"]
    set_login: TypedStatement<(String, UserId), ()>,
    #[sql = "UPDATE users SET email = ? WHERE id = ?"]
    set_email: TypedStatement<(Option<String>, UserId), ()>,
    #[sql = "UPDATE users SET password_hash = ? WHERE id = ?"]
    set_password_hash: TypedStatement<(String, UserId), ()>,

    #[sql = "INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)"]
    insert_session: TypedStatement<(String, UserId, Timestamp, Timestamp), ()>,
    #[sql = "DELETE FROM sessions WHERE expires_at <= ?"]
    delete_expired_sessions: TypedStatement<(Timestamp,), ()>,
    #[sql = "DELETE FROM sessions WHERE id = ?"]
    delete_session: TypedStatement<(String,), ()>,
    #[sql = "DELETE FROM sessions WHERE user_id = ?"]
    delete_user_sessions: TypedStatement<(UserId,), ()>,
    #[sql = "DELETE FROM sessions WHERE user_id = ? AND id IS NOT ?"]
    delete_other_sessions: TypedStatement<(UserId, Option<String>), ()>,

    #[sql = "INSERT INTO login_tokens (id, user_id, expires_at) VALUES (?, ?, ?)"]
    insert_login_token: TypedStatement<(String, UserId, Timestamp), ()>,
    #[sql = "DELETE FROM login_tokens WHERE user_id = ? AND used_at IS NULL"]
    delete_pending_login_tokens: TypedStatement<(UserId,), ()>,
    #[sql = "UPDATE login_tokens SET used_at = ? WHERE id = ?"]
    use_login_token: TypedStatement<(Timestamp, String), ()>,
}

impl Database {
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn list_users(&self) -> Result<Vec<UserRecord>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.users.list.bind(())?;

            while let Some(row) = stmt.next()? {
                out.push(row.into_record()?);
            }

            Ok(out)
        });

        result.await?
    }

    /// Each user's unused login link that has not expired at `now`.
    #[tracing::instrument(skip(self))]
    pub(crate) async fn pending_login_links(
        &self,
        now: Timestamp,
    ) -> Result<Vec<PendingLoginLink>> {
        let mut s = self.inner.clone().shared().await?;

        let result = spawn_blocking(move || {
            let mut out = Vec::new();
            let mut stmt = s.users.pending_login_links.bind((now,))?;

            while let Some(row) = stmt.next()? {
                out.push(row);
            }

            Ok(out)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn user_by_id(&self, id: UserId) -> Result<Option<UserRecord>> {
        let mut s = self.inner.clone().shared().await?;
        spawn_blocking(move || s.users.by_id(id)).await?
    }

    /// The owner of data that names no user: root, or the first administrator
    /// if root was renamed.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn default_owner(&self) -> Result<UserId> {
        let mut s = self.inner.clone().shared().await?;
        let result = spawn_blocking(move || s.users.default_owner.query()?.first());
        result
            .await??
            .ok_or_else(|| anyhow!("There is no administrator"))
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn user_by_login(&self, login: &str) -> Result<Option<UserRecord>> {
        let mut s = self.inner.clone().shared().await?;
        let login = login.to_owned();
        spawn_blocking(move || s.users.by_login(&login)).await?
    }

    /// Looks up a user by login name, or failing that by email.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn user_by_login_or_email(&self, name: &str) -> Result<Option<UserRecord>> {
        let mut s = self.inner.clone().shared().await?;
        let login = name.trim().to_owned();
        let email = auth::normalize_email(name);

        let result = spawn_blocking(move || {
            if let Some(user) = s.users.by_login(&login)? {
                return Ok(Some(user));
            }

            s.users.by_email(&email)
        });

        result.await?
    }

    /// Looks up a user by an already normalized email.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn user_by_email(&self, email: &str) -> Result<Option<UserRecord>> {
        let mut s = self.inner.clone().shared().await?;
        let email = email.to_owned();
        spawn_blocking(move || s.users.by_email(&email)).await?
    }

    /// The user owning an unexpired session.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn session_user(
        &self,
        session_id: &str,
        now: Timestamp,
    ) -> Result<Option<UserRecord>> {
        let mut s = self.inner.clone().shared().await?;
        let session_id = session_id.to_owned();

        let result = spawn_blocking(move || {
            s.users
                .by_session
                .bind((session_id, now))?
                .first()?
                .map(UserRow::into_record)
                .transpose()
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn create_user(
        &self,
        login: &str,
        email: Option<&str>,
        role: UserRole,
        now: Timestamp,
    ) -> Result<Result<UserRecord, Conflict>> {
        let mut s = self.inner.clone().exclusive().await?;
        let login = login.to_owned();
        let email = email.map(str::to_owned);

        let result = spawn_blocking(move || {
            if let Some(conflict) = s.users.conflict(None, Some(&login), email.as_deref())? {
                return Ok(Err(conflict));
            }

            let id = s
                .users_write
                .insert
                .bind((login.as_str(), email.as_deref(), role.as_str(), now))?
                .first()?
                .ok_or_else(|| anyhow!("Inserting a user returned no id"))?;

            let user = s
                .users
                .by_id(id)?
                .ok_or_else(|| anyhow!("Created user {id} not found"))?;

            Ok(Ok(user))
        });

        result.await?
    }

    /// Returns the updated user, or `None` if there is no such user.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_user_role(
        &self,
        id: UserId,
        role: UserRole,
    ) -> Result<Option<UserRecord>> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.users_write.set_role.execute((role.as_str(), id))?;
            s.users.by_id(id)
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn delete_user(&self, id: UserId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.users_write.delete.execute((id,))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_user_login(
        &self,
        id: UserId,
        login: &str,
    ) -> Result<Result<Option<UserRecord>, Conflict>> {
        let mut s = self.inner.clone().exclusive().await?;
        let login = login.to_owned();

        let result = spawn_blocking(move || {
            if let Some(conflict) = s.users.conflict(Some(id), Some(&login), None)? {
                return Ok(Err(conflict));
            }

            s.users_write.set_login.execute((login.as_str(), id))?;
            Ok(Ok(s.users.by_id(id)?))
        });

        result.await?
    }

    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn set_user_email(
        &self,
        id: UserId,
        email: Option<&str>,
    ) -> Result<Result<Option<UserRecord>, Conflict>> {
        let mut s = self.inner.clone().exclusive().await?;
        let email = email.map(str::to_owned);

        let result = spawn_blocking(move || {
            if let Some(conflict) = s.users.conflict(Some(id), None, email.as_deref())? {
                return Ok(Err(conflict));
            }

            s.users_write.set_email.execute((email.as_deref(), id))?;
            Ok(Ok(s.users.by_id(id)?))
        });

        result.await?
    }

    /// Sets a user's password and ends their sessions other than `keep`.
    #[tracing::instrument(skip(self, hash, keep))]
    pub(crate) async fn set_user_password_hash(
        &self,
        id: UserId,
        hash: &str,
        keep: Option<&str>,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;
        let hash = hash.to_owned();
        let keep = keep.map(str::to_owned);

        let result = spawn_blocking(move || {
            s.users_write
                .set_password_hash
                .execute((hash.as_str(), id))?;
            s.users_write
                .delete_other_sessions
                .execute((id, keep.as_deref()))?;
            Ok(())
        });

        result.await?
    }

    /// Stores a new session, pruning expired ones.
    #[tracing::instrument(skip(self, session_id))]
    pub(crate) async fn create_session(
        &self,
        session_id: &str,
        user_id: UserId,
        now: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;
        let session_id = session_id.to_owned();
        let expires_at = Timestamp::from_jiff(auth::session_expiry(now.into_jiff()));

        let result = spawn_blocking(move || {
            s.users_write.delete_expired_sessions.execute((now,))?;
            s.users_write.insert_session.execute((
                session_id.as_str(),
                user_id,
                now,
                expires_at,
            ))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self, session_id))]
    pub(crate) async fn delete_session(&self, session_id: &str) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;
        let session_id = session_id.to_owned();

        let result = spawn_blocking(move || {
            s.users_write
                .delete_session
                .execute((session_id.as_str(),))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self))]
    pub(crate) async fn delete_user_sessions(&self, user_id: UserId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.users_write.delete_user_sessions.execute((user_id,))?;
            Ok(())
        });

        result.await?
    }

    /// Stores a new login token for a user, replacing their unused ones.
    #[tracing::instrument(skip(self, token))]
    pub(crate) async fn create_login_token(
        &self,
        token: &str,
        user_id: UserId,
        expires_at: Timestamp,
    ) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;
        let token = token.to_owned();

        let result = spawn_blocking(move || {
            s.users_write
                .delete_pending_login_tokens
                .execute((user_id,))?;
            s.users_write
                .insert_login_token
                .execute((token.as_str(), user_id, expires_at))?;
            Ok(())
        });

        result.await?
    }

    #[tracing::instrument(skip(self))]
    pub(crate) async fn revoke_login_tokens(&self, user_id: UserId) -> Result<()> {
        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            s.users_write
                .delete_pending_login_tokens
                .execute((user_id,))?;
            Ok(())
        });

        result.await?
    }

    /// The user a login token would sign in, if it can still be used.
    #[tracing::instrument(skip(self, token))]
    pub(crate) async fn login_token_user(
        &self,
        token: &str,
        now: Timestamp,
    ) -> Result<Result<UserRecord, TokenError>> {
        let mut s = self.inner.clone().shared().await?;
        let token = token.to_owned();

        let result = spawn_blocking(move || {
            let Some(row) = s.users.login_token.bind((token,))?.first()? else {
                return Ok(Err(TokenError::NotFound));
            };

            if let Err(e) = row.check(now) {
                return Ok(Err(e));
            }

            Ok(s.users.by_id(row.user_id)?.ok_or(TokenError::NotFound))
        });

        result.await?
    }

    /// Uses up a login token: sets the user's password and replaces their
    /// sessions with a new one.
    #[tracing::instrument(skip(self, token, password_hash, session_id))]
    pub(crate) async fn redeem_login_token(
        &self,
        token: &str,
        password_hash: &str,
        session_id: &str,
        now: Timestamp,
    ) -> Result<Result<UserRecord, TokenError>> {
        let mut s = self.inner.clone().exclusive().await?;
        let token = token.to_owned();
        let password_hash = password_hash.to_owned();
        let session_id = session_id.to_owned();
        let expires_at = Timestamp::from_jiff(auth::session_expiry(now.into_jiff()));

        let result = spawn_blocking(move || {
            let Some(row) = s.users.login_token.bind((token.as_str(),))?.first()? else {
                return Ok(Err(TokenError::NotFound));
            };

            if let Err(e) = row.check(now) {
                return Ok(Err(e));
            }

            let Some(user) = s.users.by_id(row.user_id)? else {
                return Ok(Err(TokenError::NotFound));
            };

            s.users_write
                .use_login_token
                .execute((now, token.as_str()))?;
            s.users_write
                .set_password_hash
                .execute((password_hash.as_str(), user.id))?;
            s.users_write.delete_user_sessions.execute((user.id,))?;
            s.users_write.insert_session.execute((
                session_id.as_str(),
                user.id,
                now,
                expires_at,
            ))?;

            Ok(Ok(UserRecord {
                password_hash: Some(password_hash),
                ..user
            }))
        });

        result.await?
    }

    /// The key that signs session cookies, generated and stored on first use.
    #[tracing::instrument(skip(self))]
    pub(crate) async fn session_key(&self) -> Result<[u8; 32]> {
        use base64::Engine as _;
        use base64::engine::general_purpose::STANDARD;

        let mut s = self.inner.clone().exclusive().await?;

        let result = spawn_blocking(move || {
            if let Some(stored) = s.get_config("session_key")? {
                let bytes = STANDARD.decode(stored.trim())?;

                return <[u8; 32]>::try_from(bytes.as_slice())
                    .map_err(|_| anyhow!("Stored session key is not 32 bytes"));
            }

            let key = auth::generate_session_key();
            s.set_config("session_key", STANDARD.encode(key))?;
            Ok(key)
        });

        result.await?
    }
}
