-- Per-remote override of which sync kinds (api::SyncKindSet bitmask) a remote
-- contributes. NULL means inherit the global per-source default from the config
-- table; existing rows therefore inherit, preserving current behaviour.
ALTER TABLE show_remotes ADD COLUMN sync_kinds INTEGER;

ALTER TABLE movie_remotes ADD COLUMN sync_kinds INTEGER;
