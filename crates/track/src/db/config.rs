use api::{FilterRules, Locale, SourceSyncKinds};

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
