use anyhow::{Result, anyhow};
use api::UserId;
use auth::Role;
use auth::sqll015::SqllStore;
use sqll::{Row, Statements, TypedStatement};
use tokio::task::spawn_blocking;

use super::{Database, InnerRead, InnerWrite};

/// A user as a backup names them: by login, or by email without one.
#[derive(Debug, Clone)]
pub(crate) struct UserRecord {
    pub(crate) login: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) role: Role,
}

#[derive(Row)]
struct UserRow {
    id: UserId,
    login: Option<String>,
    email: Option<String>,
    role: String,
}

impl UserRow {
    fn into_record(self) -> Result<UserRecord> {
        let role = self
            .role
            .parse()
            .map_err(|_| anyhow!("User {} has unknown role {:?}", self.id, self.role))?;

        Ok(UserRecord {
            login: self.login,
            email: self.email,
            role,
        })
    }
}

#[derive(Statements)]
#[sql(read_only)]
pub(super) struct Read {
    #[sql = "SELECT id, login, email, role FROM users ORDER BY id"]
    list: TypedStatement<(), UserRow>,
    // Root, or the first administrator if root was renamed.
    #[sql = "SELECT id FROM users WHERE role = 'admin' ORDER BY login IS NOT 'root', id LIMIT 1"]
    default_owner: TypedStatement<(), UserId>,
}

impl Read {
    pub(super) fn list(&mut self) -> Result<Vec<UserRecord>> {
        let mut out = Vec::new();
        let mut stmt = self.list.bind(())?;

        while let Some(row) = stmt.next().transpose()? {
            out.push(row.into_record()?);
        }

        Ok(out)
    }
}

impl AsMut<auth::sqll015::Read> for InnerRead {
    fn as_mut(&mut self) -> &mut auth::sqll015::Read {
        &mut self.auth
    }
}

impl AsMut<auth::sqll015::Write> for InnerWrite {
    fn as_mut(&mut self) -> &mut auth::sqll015::Write {
        &mut self.auth_write
    }
}

/// The store watchword keeps users, sessions and links in, sharing the
/// database's connections.
pub(crate) type AuthStore = SqllStore<InnerRead, InnerWrite>;

impl Database {
    pub(crate) fn auth_store(&self) -> AuthStore {
        SqllStore::new(self.inner.clone())
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
}

#[cfg(test)]
impl Database {
    /// Creates an active user, with a password if `password_hash` is set.
    pub(crate) async fn test_user(
        &self,
        login: &str,
        email: Option<&str>,
        role: Role,
        password_hash: Option<&str>,
    ) -> Result<UserId> {
        use auth::Store;
        use auth::store::Credential;

        let store = self.auth_store();

        let new = auth::NewUser {
            login: Some(login.to_owned()),
            email: email.map(str::to_owned),
            role,
        };

        let user = store
            .create_user(&new, jiff::Timestamp::now())
            .await?
            .map_err(|conflict| anyhow!("{login}: {conflict:?} taken"))?;

        match password_hash {
            Some(hash) => {
                let credential = Credential::Password(hash.to_owned());
                store.set_credential(user.id, &credential, None).await?;
            }
            None => {
                store.activate(user.id).await?;
            }
        }

        Ok(UserId::new(user.id.0))
    }

    /// Creates the administrator `root` with the password `root`, as fresh
    /// databases had before yew-auth.
    pub(crate) async fn test_root(&self) -> Result<UserId> {
        self.test_user("root", None, Role::Admin, Some(ROOT_HASH))
            .await
    }
}

/// The bcrypt hash of `root`.
#[cfg(test)]
pub(crate) const ROOT_HASH: &str = "$2y$12$jxThPHsK8E/RT.IrJcuNfO6Deoc5a7DkfUZ0cLhM2TZsbPT0MvIwO";
