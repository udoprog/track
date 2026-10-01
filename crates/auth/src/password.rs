pub const MIN_PASSWORD_LEN: usize = 8;

#[derive(Debug, thiserror::Error)]
#[error("failed to hash password")]
pub struct PasswordError(#[source] bcrypt::BcryptError);

/// Hashes with bcrypt at its default cost, which keeps hashes interchangeable
/// with territory's.
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(PasswordError)
}

/// A malformed hash verifies as false.
pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

/// Returns a message for the user when the password is not acceptable.
pub fn validate_password(password: &str) -> Option<&'static str> {
    if password.len() < MIN_PASSWORD_LEN {
        return Some("Must be at least 8 characters.");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_round_trip() {
        let hash = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &hash));
        assert!(!verify_password("wrong horse", &hash));
        assert!(!verify_password("correct horse", "not a hash"));
    }

    #[test]
    fn validation() {
        assert!(validate_password("1234567").is_some());
        assert!(validate_password("12345678").is_none());
    }
}
