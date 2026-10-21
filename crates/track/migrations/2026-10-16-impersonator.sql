-- Records the admin behind an impersonated session, as watchword's SqllStore
-- expects as of d463305 (docs/sqll/impersonator.sql), watchword's migration
-- watchword-0002-sessions-impersonator. Existing sessions are not
-- impersonated.
ALTER TABLE sessions
ADD COLUMN impersonator_id INTEGER REFERENCES users (id) ON DELETE CASCADE;
