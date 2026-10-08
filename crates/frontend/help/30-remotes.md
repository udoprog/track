id: remotes
title: Remotes and syncing

A *remote* is an identifier for a show, movie or person on another site:
TMDB, TheTVDB, TVmaze, IMDb, AniDB, XEM or a scene name. Track keeps its own
copy of everything and fills it in by syncing from the remotes.

## What comes from where

Each source provides some kinds of data:

| Source | Base | Dates | Credits |
| - | - | - | - |
| TMDB | yes | yes | yes |
| TheTVDB | yes | yes | |
| TVmaze | | yes | |

*Base* is the title, overview, seasons and episodes, *Dates* are air and
release dates, and *Credits* are the cast and crew. Base and credits come from
the highest source in **Settings > Sources & dates > Sync sources**; dates are
merged from every source.

AniDB, XEM and scene remotes provide none of these:

- **XEM** (thexem.info) maps a show's episodes between numberings and knows
  other names for it and its seasons; see [Other numberings and
  names](numberings). Syncing finds a show's XEM entry through its TheTVDB or
  AniDB id and adds it as a remote; while it is turned on, syncing keeps the
  mappings and names up to date. It links to the show's XEM page once syncing
  has found it on XEM's website.
- **AniDB** links to the show's AniDB entry. A show can have several, one per
  cour.
- **Scene** is the name release groups use for the show. It has no page to
  link to.

## Editing remotes

Open a show or movie's ![Settings](button:cog-6-tooth) settings and choose
![Edit remotes](icon:identification) **Edit remotes**. Each remote can be
turned off, edited, removed or dragged to change its order. Its **Syncs**
switches choose which kinds of data it provides for this item; **Use
default** goes back to the order in Settings. ![Clear
cache](icon:arrow-path) **Clear cache** forgets what was fetched and syncs it
again.

TMDB, TheTVDB, TVmaze and AniDB identifiers are numbers; IMDb identifiers
look like `tt1234567`. An XEM identifier is where XEM was found and the id
there, such as `tvdb/424536`; its slug is XEM's own show id, such as `6743`
for `thexem.info/xem/show/6743`.

## Syncing

Tracked items sync on their own when **Automatic sync** is on in
**Settings > Sync**. To sync one now, press ![Sync](button:arrow-path) on its
page. An episode has **Sync episode** under its more actions.

Every sync is a task in the ![Queue](icon:queue-list) **Queue**, which shows
what runs next, what is done and what failed. Administrators can run a task
right away, remove it, or **Sync all**.
