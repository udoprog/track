use api::{FilterRules, Locale, PreferenceValue, SourceSyncKinds};
use sqll::{FromColumn, Statement, ty};

/// A preference read from a `*_config` table: the row's JSON value, NULL when
/// there is no row. No row, or a value that does not read, is the default.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct Pref<T>(pub(crate) T);

impl<T> FromColumn<'_> for Pref<T>
where
    T: PreferenceValue + Default,
{
    type Type = ty::Nullable<ty::Text>;

    fn from_column(stmt: &Statement, index: Self::Type) -> sqll::Result<Self> {
        let Some(json) = Option::<String>::from_column(stmt, index)? else {
            return Ok(Self(T::default()));
        };

        match T::from_json(&json) {
            Some(value) => Ok(Self(value)),
            None => {
                tracing::warn!(json, "Skipping unreadable preference value");
                Ok(Self(T::default()))
            }
        }
    }
}

/// Serialize a collection of filter rules (release or air-date) for storage in a text column.
pub(super) fn encode_filter_rules(rules: &FilterRules) -> String {
    serde_json::to_string(rules).unwrap_or_else(|_| "[]".to_string())
}

/// Parse filter rules previously written by [`encode_filter_rules`].
pub(super) fn decode_filter_rules(s: &str) -> Option<FilterRules> {
    serde_json::from_str(s).ok()
}

/// Serialize the global per-source sync-kind defaults for storage in a text column.
pub(super) fn encode_sync_kinds(kinds: &[SourceSyncKinds]) -> String {
    serde_json::to_string(kinds).unwrap_or_else(|_| "[]".to_string())
}

/// Parse global per-source sync-kind defaults written by [`encode_sync_kinds`].
pub(super) fn decode_sync_kinds(s: &str) -> Option<Vec<SourceSyncKinds>> {
    serde_json::from_str(s).ok()
}

/// Serialize the locales the sync path populates, for storage in a text column.
/// Each entry is its string form (`"default"` / `"eng"` / `"en-US"`).
pub(super) fn encode_sync_languages(languages: &[Locale]) -> String {
    serde_json::to_string(languages).unwrap_or_else(|_| "[]".to_string())
}

/// Parse sync locales written by [`encode_sync_languages`].
pub(super) fn decode_sync_languages(s: &str) -> Option<Vec<Locale>> {
    serde_json::from_str(s).ok()
}
