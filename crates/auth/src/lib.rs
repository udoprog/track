//! Storage-agnostic authentication primitives: roles, password hashing,
//! session ids and login tokens, signed session cookies and Cloudflare Access.

pub mod cloudflare;
mod cookie;
mod password;
mod session;

pub use self::cookie::{generate_session_key, session_cookie, verify_session_cookie};
pub use self::password::{
    MIN_PASSWORD_LEN, PasswordError, hash_password, validate_password, verify_password,
};
pub use self::session::{
    LOGIN_TOKEN_LIFETIME, SESSION_LIFETIME, SESSION_LIFETIME_DAYS, is_expired, login_token_expiry,
    new_login_token, new_session_id, session_expiry,
};

use std::fmt;
use std::str::FromStr;

/// Trims and lowercases an email so lookups and comparisons are
/// case-insensitive.
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserRole {
    Admin,
    Regular,
}

impl UserRole {
    pub fn as_str(self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::Regular => "regular",
        }
    }
}

impl fmt::Display for UserRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown user role")]
pub struct UnknownUserRole;

impl FromStr for UserRole {
    type Err = UnknownUserRole;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(UserRole::Admin),
            "regular" => Ok(UserRole::Regular),
            _ => Err(UnknownUserRole),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_text_round_trip() {
        for role in [UserRole::Admin, UserRole::Regular] {
            assert_eq!(role.as_str().parse::<UserRole>(), Ok(role));
        }

        assert_eq!("Admin".parse::<UserRole>(), Err(UnknownUserRole));
    }

    #[test]
    fn email_normalization() {
        assert_eq!(normalize_email("  Foo@Example.COM \n"), "foo@example.com");
    }
}
