use api::{AirDateFilter, Locale, ReleaseFilter, SourceSyncKinds};

/// Serialize air-date filters for storage in a text column.
pub(super) fn encode_air_date_filters(filters: &[AirDateFilter]) -> String {
    serde_json::to_string(filters).unwrap_or_else(|_| "[]".to_string())
}

/// Parse air-date filters previously written by [`encode_air_date_filters`].
pub(super) fn decode_air_date_filters(s: &str) -> Option<Vec<AirDateFilter>> {
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

/// Serialize release filters for storage in a text column.
pub(super) fn encode_release_filters(filters: &[ReleaseFilter]) -> String {
    serde_json::to_string(filters).unwrap_or_else(|_| "[]".to_string())
}

/// Parse release filters previously written by [`encode_release_filters`].
pub(super) fn decode_release_filters(s: &str) -> Option<Vec<ReleaseFilter>> {
    serde_json::from_str(s).ok()
}
