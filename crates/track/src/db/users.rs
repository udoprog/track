use anyhow::{Result, anyhow};
use api::UserId;
use auth::Role;
use sqll::{Row, Statements, TypedStatement};

use super::Database;

/// A user as a backup names them: by login, or by email without one.
#[derive(Debug, Clone, Row)]
pub(crate) struct UserRecord {
    pub(crate) login: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) role: Role,
}

#[derive(Statements)]
#[sql(read_only)]
pub(super) struct Read {
    #[sql = "SELECT login, email, role FROM users ORDER BY id"]
    list: TypedStatement<(), UserRecord>,
    // Root, or the first administrator if root was renamed.
    #[sql = "SELECT id FROM users WHERE role = ? ORDER BY login IS NOT 'root', id LIMIT 1"]
    default_owner: TypedStatement<Role, UserId>,
}

impl Read {
    pub(super) fn list(&mut self) -> Result<Vec<UserRecord>> {
        let mut out = Vec::new();
        let mut stmt = self.list.bind(())?;

        while let Some(row) = stmt.next().transpose()? {
            out.push(row);
        }

        Ok(out)
    }
}

impl Database {
    /// The owner of data that names no user: root, or the first administrator
    /// if root was renamed.
    #[tracing::instrument(skip(self), ret(level = "trace"))]
    pub(crate) async fn default_owner(&self) -> Result<UserId> {
        let result = self
            .inner
            .read(move |s| s.users.default_owner.bind(Role::Admin)?.first());
        result
            .await??
            .ok_or_else(|| anyhow!("There is no administrator"))
    }
}

#[cfg(test)]
impl Database {
    /// Creates an active user, who signs in with `password` if it is set, as
    /// watchword's `Auth::test_user` does.
    pub(crate) async fn test_user(
        &self,
        login: &str,
        email: Option<&str>,
        role: Role,
        password: Option<&str>,
    ) -> Result<UserId> {
        use auth::Store;

        let auth = crate::entry::build_auth(self, &[]).await?;

        let new = auth::NewUser {
            login: Some(login.to_owned()),
            email: email.map(str::to_owned),
            role,
        };

        let user = match password {
            Some(password) => auth.test_user(&new, password).await?,
            None => {
                let user = auth.import_user(&new).await?;
                self.auth_store().activate(user.id).await?;
                user
            }
        };

        Ok(user.id)
    }

    /// Creates the administrator `root` with the password `root`, as fresh
    /// databases had before 2026-10-15-yew-auth.sql moved users onto
    /// watchword.
    pub(crate) async fn test_root(&self) -> Result<UserId> {
        self.test_user("root", None, Role::Admin, Some("root"))
            .await
    }
}
