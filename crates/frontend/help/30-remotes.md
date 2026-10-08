id: remotes
title: Remotes and syncing

A *remote* is an identifier for a show, movie or person on another site:
TMDB, TheTVDB, TVmaze or IMDb. Track keeps its own copy of everything and
fills it in by syncing from the remotes.

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

## Editing remotes

Open a show or movie's ![Settings](button:cog-6-tooth) settings and choose
![Edit remotes](icon:identification) **Edit remotes**. Each remote can be
turned off, edited, removed or dragged to change its order. Its **Syncs**
switches choose which kinds of data it provides for this item; **Use
default** goes back to the order in Settings. ![Clear
cache](icon:arrow-path) **Clear cache** forgets what was fetched and syncs it
again.

TMDB, TheTVDB and TVmaze identifiers are numbers; IMDb identifiers look like
`tt1234567`.

## Syncing

Tracked items sync on their own when **Automatic sync** is on in
**Settings > Sync**. To sync one now, press ![Sync](button:arrow-path) on its
page. An episode has **Sync episode** under its more actions.

Every sync is a task in the ![Queue](icon:queue-list) **Queue**, which shows
what runs next, what is done and what failed. Administrators can run a task
right away, remove it, or **Sync all**.
