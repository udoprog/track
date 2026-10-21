//! Track's watchword [`Store`], over the `users`, `sessions`, `auth_tokens`
//! and `config` tables of the base schema. These are the tables watchword's
//! former `SqllStore` kept, and the statements are the ones it ran, so the
//! stored data reads and writes as it did. Timestamps are INTEGER
//! milliseconds since the Unix epoch.
//!
//! The statements nest in [`InnerRead`] and [`InnerWrite`], so the store
//! shares the database's pool and its one write connection.
//!
//! [`InnerRead`]: super::InnerRead
//! [`InnerWrite`]: super::InnerWrite

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use auth::store::{self, Credential, Session, Store, StoreError, Token, TokenKind, User};
use auth::{Access, Conflict, Method, NewUser, Role, UserChange, UserId};
use jiff::Timestamp;
use sqll::{Pool, Row, Statements, TypedStatement};

use super::{Database, InnerRead, InnerWrite};

/// The store watchword keeps users, sessions and links in. Clones share the
/// database's pool.
#[derive(Clone)]
pub(crate) struct AuthStore {
    pool: Arc<Pool<InnerRead, InnerWrite>>,
}

impl Database {
    pub(crate) fn auth_store(&self) -> AuthStore {
        AuthStore {
            pool: self.inner.clone(),
        }
    }
}

impl AuthStore {
    /// Runs `f` on a read connection.
    async fn read<T>(
        &self,
        f: impl FnOnce(&mut Read) -> Result<T> + Send + 'static,
    ) -> store::Result<T>
    where
        T: Send + 'static,
    {
        let result = self.pool.read(move |s| f(&mut s.auth)).await;
        result.map_err(StoreError::new)?.map_err(StoreError::new)
    }

    /// Runs `f` on the write connection in one `BEGIN IMMEDIATE`
    /// transaction, which commits when `f` returns `Ok` and rolls back when
    /// it fails or panics.
    async fn write<T>(
        &self,
        f: impl FnOnce(&mut Write) -> Result<T> + Send + 'static,
    ) -> store::Result<T>
    where
        T: Send + 'static,
    {
        let result = self
            .pool
            .write(move |s| s.transaction(|w, _| f(&mut w.auth_write)))
            .await;

        result.map_err(StoreError::new)?.map_err(StoreError::new)
    }
}

#[derive(Statements)]
#[sql(read_only)]
pub(super) struct Read {
    #[sql = "SELECT id, login, email, role, access, method, password_hash, google_sub, created_at"]
    #[sql = "FROM users WHERE id = ?"]
    user: TypedStatement<(UserId,), UserRow>,
    #[sql = "SELECT id, login, email, role, access, method, password_hash, google_sub, created_at"]
    #[sql = "FROM users WHERE email = ?"]
    user_by_email: TypedStatement<(String,), UserRow>,
    #[sql = "SELECT id, login, email, role, access, method, password_hash, google_sub, created_at"]
    #[sql = "FROM users WHERE login = ?"]
    user_by_login: TypedStatement<(String,), UserRow>,
    #[sql = "SELECT id, login, email, role, access, method, password_hash, google_sub, created_at"]
    #[sql = "FROM users WHERE google_sub = ?"]
    user_by_google_sub: TypedStatement<(String,), UserRow>,
    #[sql = "SELECT u.id, u.login, u.email, u.role, u.access, u.method, u.password_hash,"]
    #[sql = "u.google_sub, u.created_at,"]
    #[sql = "(SELECT MAX(t.expires_at) FROM auth_tokens t"]
    #[sql = " WHERE t.user_id = u.id AND t.used_at IS NULL AND t.expires_at > ?) AS link"]
    #[sql = "FROM users u ORDER BY u.id"]
    users: TypedStatement<(i64,), UserLinkRow>,
    #[sql = "SELECT COUNT(*) FROM users"]
    user_count: TypedStatement<(), i64>,
    #[sql = "SELECT id, user_id, impersonator_id, created_at, expires_at FROM sessions WHERE id = ?"]
    session: TypedStatement<(String,), SessionRow>,
    #[sql = "SELECT id, user_id, kind, expires_at, used_at FROM auth_tokens WHERE id = ?"]
    token: TypedStatement<(String,), TokenRow>,
    #[sql = "SELECT value FROM config WHERE key = ?"]
    config: TypedStatement<(String,), String>,
}

/// The store's write statements, with its reads for use inside a
/// transaction.
#[derive(Statements)]
pub(super) struct Write {
    #[sql(statements)]
    read: Read,

    #[sql = "INSERT INTO users (login, email, role, access, method, created_at)"]
    #[sql = "VALUES (?, ?, ?, 'pending', 'password', ?) RETURNING id"]
    insert_user: TypedStatement<(Option<String>, Option<String>, Role, i64), UserId>,
    #[sql = "UPDATE users SET login = ? WHERE id = ?"]
    set_login: TypedStatement<(Option<String>, UserId), ()>,
    #[sql = "UPDATE users SET email = ? WHERE id = ?"]
    set_email: TypedStatement<(Option<String>, UserId), ()>,
    #[sql = "UPDATE users SET role = ? WHERE id = ?"]
    set_role: TypedStatement<(Role, UserId), ()>,
    #[sql = "DELETE FROM users WHERE id = ?"]
    delete_user: TypedStatement<(UserId,), ()>,
    #[sql = "UPDATE users SET method = 'password', password_hash = ?, google_sub = NULL,"]
    #[sql = "access = 'active' WHERE id = ?"]
    set_password: TypedStatement<(String, UserId), ()>,
    #[sql = "UPDATE users SET method = 'google', password_hash = NULL, google_sub = ?,"]
    #[sql = "email = ?, access = 'active' WHERE id = ?"]
    set_google: TypedStatement<(String, String, UserId), ()>,
    #[sql = "UPDATE users SET google_sub = ? WHERE id = ?"]
    set_google_sub: TypedStatement<(String, UserId), ()>,
    #[sql = "UPDATE users SET access = ? WHERE id = ?"]
    set_access: TypedStatement<(Access, UserId), ()>,
    #[sql = "UPDATE users SET access = 'active' WHERE id = ? AND access = 'pending'"]
    activate: TypedStatement<(UserId,), ()>,
    #[sql = "UPDATE users SET access = 'revoked', password_hash = NULL WHERE id = ?"]
    revoke: TypedStatement<(UserId,), ()>,

    #[sql = "INSERT INTO sessions (id, user_id, impersonator_id, created_at, expires_at)"]
    #[sql = "VALUES (?, ?, ?, ?, ?)"]
    insert_session: TypedStatement<(String, UserId, Option<UserId>, i64, i64), ()>,
    #[sql = "DELETE FROM sessions WHERE id = ?"]
    delete_session: TypedStatement<(String,), ()>,
    #[sql = "DELETE FROM sessions WHERE user_id = ? AND id IS NOT ?"]
    delete_sessions: TypedStatement<(UserId, Option<String>), ()>,
    #[sql = "DELETE FROM sessions WHERE impersonator_id = ?"]
    delete_impersonations: TypedStatement<(UserId,), ()>,
    #[sql = "DELETE FROM sessions WHERE expires_at <= ?"]
    delete_expired_sessions: TypedStatement<(i64,), ()>,

    #[sql = "INSERT INTO auth_tokens (id, user_id, kind, expires_at, used_at) VALUES (?, ?, ?, ?, ?)"]
    insert_token: TypedStatement<(String, UserId, String, i64, Option<i64>), ()>,
    #[sql = "UPDATE auth_tokens SET used_at = ? WHERE user_id = ? AND used_at IS NULL"]
    use_up_tokens: TypedStatement<(i64, UserId), ()>,
    #[sql = "UPDATE auth_tokens SET used_at = ? WHERE id = ? AND used_at IS NULL"]
    use_token: TypedStatement<(i64, String), ()>,
    #[sql = "DELETE FROM auth_tokens WHERE user_id = ?"]
    delete_tokens: TypedStatement<(UserId,), ()>,
    #[sql = "DELETE FROM auth_tokens WHERE expires_at <= ?"]
    delete_expired_tokens: TypedStatement<(i64,), ()>,

    #[sql = "INSERT INTO config (key, value) VALUES (?, ?)"]
    #[sql = "ON CONFLICT (key) DO UPDATE SET value = excluded.value"]
    set_config: TypedStatement<(String, String), ()>,
}

impl Read {
    fn user(&mut self, id: UserId) -> Result<Option<User>> {
        let row = self.user.bind((id,))?.first()?;
        row.map(User::try_from).transpose()
    }

    fn user_by_email(&mut self, email: &str) -> Result<Option<User>> {
        let row = self.user_by_email.bind((email,))?.first()?;
        row.map(User::try_from).transpose()
    }

    fn user_by_login(&mut self, login: &str) -> Result<Option<User>> {
        let row = self.user_by_login.bind((login,))?.first()?;
        row.map(User::try_from).transpose()
    }

    fn user_by_google_sub(&mut self, sub: &str) -> Result<Option<User>> {
        let row = self.user_by_google_sub.bind((sub,))?.first()?;
        row.map(User::try_from).transpose()
    }

    /// The conflict, if `login` or `email` belongs to a user other than
    /// `id`. Run inside the write transaction, so nothing changes between
    /// this check and the write it guards.
    fn conflict(
        &mut self,
        id: Option<UserId>,
        login: Option<&str>,
        email: Option<&str>,
    ) -> Result<Option<Conflict>> {
        if let Some(login) = login
            && let Some(user) = self.user_by_login(login)?
            && Some(user.id) != id
        {
            return Ok(Some(Conflict::Login));
        }

        if let Some(email) = email
            && let Some(user) = self.user_by_email(email)?
            && Some(user.id) != id
        {
            return Ok(Some(Conflict::Email));
        }

        Ok(None)
    }

    fn token(&mut self, id: &str) -> Result<Option<Token>> {
        let row = self.token.bind((id,))?.first()?;
        row.map(Token::try_from).transpose()
    }
}

impl Write {
    /// Sets the credential and activates the user, without touching
    /// sessions. Returns whether the user exists. A Google subject or email
    /// that belongs to another user fails on the unique constraints.
    fn set_credential(&mut self, id: UserId, cred: &Credential) -> Result<bool> {
        let changed = match cred {
            Credential::Password(hash) => self.set_password.execute((hash.as_str(), id))?,
            Credential::Google { sub, email } => {
                self.set_google
                    .execute((sub.as_str(), email.as_str(), id))?
            }
        };

        Ok(changed > 0)
    }

    fn delete_sessions(&mut self, user: UserId, keep: Option<&str>) -> Result<()> {
        self.delete_sessions.execute((user, keep))?;
        Ok(())
    }

    fn insert_session(&mut self, session: &Session) -> Result<()> {
        self.insert_session.execute((
            session.id.as_str(),
            session.user,
            session.impersonator,
            millis(session.created_at),
            millis(session.expires_at),
        ))?;
        Ok(())
    }
}

impl Store for AuthStore {
    async fn user(&self, id: UserId) -> store::Result<Option<User>> {
        self.read(move |r| r.user(id)).await
    }

    async fn user_by_email(&self, email: &str) -> store::Result<Option<User>> {
        let email = email.to_owned();
        self.read(move |r| r.user_by_email(&email)).await
    }

    async fn user_by_login(&self, login: &str) -> store::Result<Option<User>> {
        let login = login.to_owned();
        self.read(move |r| r.user_by_login(&login)).await
    }

    async fn user_by_google_sub(&self, sub: &str) -> store::Result<Option<User>> {
        let sub = sub.to_owned();
        self.read(move |r| r.user_by_google_sub(&sub)).await
    }

    async fn users(&self, now: Timestamp) -> store::Result<Vec<(User, Option<Timestamp>)>> {
        self.read(move |r| {
            let mut users = Vec::new();
            let mut stmt = r.users.bind((millis(now),))?;

            while let Some(row) = stmt.next().transpose()? {
                let (user, link) = row.split();
                users.push((User::try_from(user)?, link.map(timestamp).transpose()?));
            }

            Ok(users)
        })
        .await
    }

    async fn user_count(&self) -> store::Result<u64> {
        self.read(|r| {
            let count = r.user_count.query()?.first()?.unwrap_or_default();
            Ok(u64::try_from(count)?)
        })
        .await
    }

    async fn create_user(
        &self,
        new: &NewUser,
        now: Timestamp,
    ) -> store::Result<Result<User, Conflict>> {
        let new = new.clone();

        self.write(move |w| {
            let (login, email) = (new.login.as_deref(), new.email.as_deref());

            if let Some(conflict) = w.read.conflict(None, login, email)? {
                return Ok(Err(conflict));
            }

            let id = w
                .insert_user
                .bind((login, email, new.role, millis(now)))?
                .first()?
                .context("Insert returned no id")?;

            let user = w.read.user(id)?.context("Inserted user is missing")?;
            Ok(Ok(user))
        })
        .await
    }

    async fn update_user(
        &self,
        id: UserId,
        change: &UserChange,
    ) -> store::Result<Result<Option<User>, Conflict>> {
        let change = change.clone();

        self.write(move |w| {
            let login = change.login.as_ref().and_then(Option::as_deref);
            let email = change.email.as_ref().and_then(Option::as_deref);

            if let Some(conflict) = w.read.conflict(Some(id), login, email)? {
                return Ok(Err(conflict));
            }

            if w.read.user(id)?.is_none() {
                return Ok(Ok(None));
            }

            if let Some(login) = &change.login {
                w.set_login.execute((login.as_deref(), id))?;
            }

            if let Some(email) = &change.email {
                w.set_email.execute((email.as_deref(), id))?;
            }

            if let Some(role) = change.role {
                w.set_role.execute((role, id))?;
            }

            Ok(Ok(w.read.user(id)?))
        })
        .await
    }

    async fn delete_user(&self, id: UserId) -> store::Result<bool> {
        // Deletes the user's sessions and links itself rather than relying
        // on the foreign keys' cascades.
        self.write(move |w| {
            w.delete_sessions(id, None)?;
            w.delete_impersonations.execute((id,))?;
            w.delete_tokens.execute((id,))?;
            Ok(w.delete_user.execute((id,))? > 0)
        })
        .await
    }

    async fn set_credential(
        &self,
        id: UserId,
        cred: &Credential,
        keep: Option<&str>,
    ) -> store::Result<bool> {
        let cred = cred.clone();
        let keep = keep.map(str::to_owned);

        self.write(move |w| {
            // Read inside the transaction, so a revocation cannot land in
            // between.
            if w.read.user(id)?.is_none_or(|u| u.access == Access::Revoked) {
                return Ok(false);
            }

            if !w.set_credential(id, &cred)? {
                return Ok(false);
            }

            w.delete_sessions(id, keep.as_deref())?;
            Ok(true)
        })
        .await
    }

    async fn bind_google_sub(&self, id: UserId, sub: &str) -> store::Result<bool> {
        let sub = sub.to_owned();

        self.write(move |w| {
            if let Some(bound) = w.read.user_by_google_sub(&sub)? {
                return Ok(bound.id == id);
            }

            let Some(user) = w.read.user(id)? else {
                return Ok(false);
            };

            if user.google_sub.is_some() {
                // Bound to another subject, since `sub` is bound to no one.
                return Ok(false);
            }

            w.set_google_sub.execute((sub.as_str(), id))?;
            Ok(true)
        })
        .await
    }

    async fn set_access(&self, id: UserId, access: Access) -> store::Result<bool> {
        self.write(move |w| Ok(w.set_access.execute((access, id))? > 0))
            .await
    }

    async fn activate(&self, id: UserId) -> store::Result<bool> {
        // One conditional UPDATE, so a revocation in between is not undone.
        self.write(move |w| Ok(w.activate.execute((id,))? > 0))
            .await
    }

    async fn revoke(&self, id: UserId, now: Timestamp) -> store::Result<bool> {
        self.write(move |w| {
            if w.revoke.execute((id,))? == 0 {
                return Ok(false);
            }

            w.use_up_tokens.execute((millis(now), id))?;
            w.delete_sessions(id, None)?;
            Ok(true)
        })
        .await
    }

    async fn create_session(&self, session: &Session) -> store::Result<()> {
        let session = session.clone();

        self.write(move |w| {
            w.delete_expired_sessions
                .execute((millis(session.created_at),))?;
            w.insert_session(&session)
        })
        .await
    }

    async fn session(&self, id: &str) -> store::Result<Option<Session>> {
        let id = id.to_owned();

        self.read(move |r| {
            let row = r.session.bind((id.as_str(),))?.first()?;
            row.map(Session::try_from).transpose()
        })
        .await
    }

    async fn delete_session(&self, id: &str) -> store::Result<()> {
        let id = id.to_owned();

        self.write(move |w| {
            w.delete_session.execute((id.as_str(),))?;
            Ok(())
        })
        .await
    }

    async fn delete_sessions(&self, user: UserId, keep: Option<&str>) -> store::Result<()> {
        let keep = keep.map(str::to_owned);
        self.write(move |w| w.delete_sessions(user, keep.as_deref()))
            .await
    }

    async fn delete_expired_sessions(&self, now: Timestamp) -> store::Result<()> {
        self.write(move |w| {
            w.delete_expired_sessions.execute((millis(now),))?;
            Ok(())
        })
        .await
    }

    async fn create_token(&self, token: &Token, now: Timestamp) -> store::Result<()> {
        let token = token.clone();

        self.write(move |w| {
            w.use_up_tokens.execute((millis(now), token.user))?;
            w.insert_token.execute((
                token.id.as_str(),
                token.user,
                token.kind.as_str(),
                millis(token.expires_at),
                token.used_at.map(millis),
            ))?;
            Ok(())
        })
        .await
    }

    async fn token(&self, id: &str) -> store::Result<Option<Token>> {
        let id = id.to_owned();
        self.read(move |r| r.token(&id)).await
    }

    async fn delete_expired_tokens(&self, before: Timestamp) -> store::Result<()> {
        self.write(move |w| {
            w.delete_expired_tokens.execute((millis(before),))?;
            Ok(())
        })
        .await
    }

    async fn use_up_tokens(&self, user: UserId, now: Timestamp) -> store::Result<()> {
        self.write(move |w| {
            w.use_up_tokens.execute((millis(now), user))?;
            Ok(())
        })
        .await
    }

    async fn redeem_token(
        &self,
        token: &str,
        cred: &Credential,
        session: &Session,
        now: Timestamp,
    ) -> store::Result<bool> {
        let token = token.to_owned();
        let cred = cred.clone();
        let session = session.clone();

        self.write(move |w| {
            // Read inside the transaction, so nothing changes in between.
            let Some(row) = w.read.token(&token)? else {
                return Ok(false);
            };

            if row.used_at.is_some() {
                return Ok(false);
            }

            // Any access: a registration link re-registers a revoked user.
            if !w.set_credential(row.user, &cred)? {
                return Ok(false);
            }

            w.use_token.execute((millis(now), token.as_str()))?;
            w.delete_sessions(row.user, None)?;
            w.insert_session(&session)?;
            Ok(true)
        })
        .await
    }

    async fn config_get(&self, key: &str) -> store::Result<Option<String>> {
        let key = key.to_owned();
        self.read(move |r| Ok(r.config.bind((key.as_str(),))?.first()?))
            .await
    }

    async fn config_set(&self, key: &str, value: &str) -> store::Result<()> {
        let (key, value) = (key.to_owned(), value.to_owned());

        self.write(move |w| {
            w.set_config.execute((key.as_str(), value.as_str()))?;
            Ok(())
        })
        .await
    }
}

#[derive(Row)]
struct UserRow {
    id: UserId,
    login: Option<String>,
    email: Option<String>,
    role: Role,
    access: Access,
    method: Method,
    password_hash: Option<String>,
    google_sub: Option<String>,
    created_at: i64,
}

impl TryFrom<UserRow> for User {
    type Error = anyhow::Error;

    fn try_from(row: UserRow) -> Result<Self> {
        Ok(User {
            id: row.id,
            login: row.login,
            email: row.email,
            role: row.role,
            access: row.access,
            method: row.method,
            password_hash: row.password_hash,
            google_sub: row.google_sub,
            created_at: timestamp(row.created_at)?,
        })
    }
}

/// A [`UserRow`] with the expiry of the user's live link.
#[derive(Row)]
struct UserLinkRow {
    id: UserId,
    login: Option<String>,
    email: Option<String>,
    role: Role,
    access: Access,
    method: Method,
    password_hash: Option<String>,
    google_sub: Option<String>,
    created_at: i64,
    link: Option<i64>,
}

impl UserLinkRow {
    fn split(self) -> (UserRow, Option<i64>) {
        let user = UserRow {
            id: self.id,
            login: self.login,
            email: self.email,
            role: self.role,
            access: self.access,
            method: self.method,
            password_hash: self.password_hash,
            google_sub: self.google_sub,
            created_at: self.created_at,
        };

        (user, self.link)
    }
}

#[derive(Row)]
struct SessionRow {
    id: String,
    user_id: UserId,
    impersonator_id: Option<UserId>,
    created_at: i64,
    expires_at: i64,
}

impl TryFrom<SessionRow> for Session {
    type Error = anyhow::Error;

    fn try_from(row: SessionRow) -> Result<Self> {
        Ok(Session {
            id: row.id,
            user: row.user_id,
            impersonator: row.impersonator_id,
            created_at: timestamp(row.created_at)?,
            expires_at: timestamp(row.expires_at)?,
        })
    }
}

#[derive(Row)]
struct TokenRow {
    id: String,
    user_id: UserId,
    kind: String,
    expires_at: i64,
    used_at: Option<i64>,
}

impl TryFrom<TokenRow> for Token {
    type Error = anyhow::Error;

    fn try_from(row: TokenRow) -> Result<Self> {
        let kind = row
            .kind
            .parse::<TokenKind>()
            .map_err(|e| anyhow!("Token {}: {e}", row.id))?;

        Ok(Token {
            id: row.id,
            user: row.user_id,
            kind,
            expires_at: timestamp(row.expires_at)?,
            used_at: row.used_at.map(timestamp).transpose()?,
        })
    }
}

/// A stored time, in milliseconds since the Unix epoch.
fn millis(timestamp: Timestamp) -> i64 {
    timestamp.as_millisecond()
}

fn timestamp(millis: i64) -> Result<Timestamp> {
    Ok(Timestamp::from_millisecond(millis)?)
}
