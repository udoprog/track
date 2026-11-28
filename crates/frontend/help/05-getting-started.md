id: getting-started
title: Getting started

A new track server starts with no users. On its first start it creates the
administrator, `root` unless the server was started with another, and logs a
*registration link* for them. Open the link to choose a password; you are
signed in once it is set.

![The registration page: the login it is for, and the password
fields](images/register.webp)

A link works once and expires after a week. An administrator who has lost
theirs, or their password, can print a new one on the server with
`track issue-link <login>`.

## API keys

Track finds and syncs shows and movies through TheTVDB and TMDB, and needs
an API key for each. Both sites hand them out for free from your account
settings there. Paste them under ![Settings](icon:cog-6-tooth)
**Settings > API keys**; until a key is set, that source is not searched or
synced. TheTVDB's subscriber PIN is optional.

![The API keys settings, with both keys filled in and
hidden](images/api-keys.webp)

## Next steps

- [Find and track](finding) the shows and movies you watch.
- [Mark them watched](watching) as you go, from the dashboard.
- [Invite the people](users) you share the server with.
- Look through [Settings](settings) for your theme, time zone and language.

The ![Help](icon:question-mark-circle) button in the toolbar opens this help
from any page, and the ![Help](icon:question-mark-circle) beside a heading
opens the part about it.
