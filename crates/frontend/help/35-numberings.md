id: numberings
title: Other numberings and names

Track numbers episodes the way the show's base source does, usually TMDB.
Other sites number some shows differently: TheTVDB may split a long TMDB
season in two, AniDB gives every cour its own entry, and release groups
follow the *scene* numbering. [XEM](remotes) maps between them, and track
shows the other numbers under each episode that has them.

## On an episode

An episode whose number differs elsewhere shows a small chip per numbering,
such as **TheTVDB S02E01**; hover it for the absolute number. A double
episode reads **S01E03+04**. An episode that lines up shows nothing extra.

When one season covers several seasons of the other numbering, a line before
the first episode of each names that season and its XEM name.

## Linking a show to XEM

XEM numbers by TheTVDB, so track has to know where a show's episodes are
there. Open the show's ![Settings](button:cog-6-tooth) settings and pick
an **Episode numbering**:

- **Automatic: same as TheTVDB** assumes the show's seasons and episodes
  are numbered like TheTVDB's. When a season's episode count differs, a
  warning links to the range editor to **Compare** them.
- **Manual ranges** maps a run of the show's episodes onto the start of a
  season in TheTVDB, AniDB, scene or another numbering.
  ![Edit ranges](button:adjustments-horizontal) **Edit ranges** opens the
  ranges. An episode outside every range shows no other numbers.

The range editor lists the show's TMDB episodes beside those of the other
numbering, with a band from each range to the episodes it maps to. Episodes
no range covers are hatched, episodes XEM doesn't know are dashed in amber,
and ranges that overlap turn red and can't be saved. In automatic mode it
shows what automatic numbering does; **Suggest from episode order** pairs
the episodes one to one and shows the result before you save it.

To edit, click an unmapped TMDB episode to start a range there, or a mapped
one to select its range. With a range selected, click two TMDB episodes to
set its first and last, and an episode on the other side to set where it
starts. The selected range's numbers can also be typed.

A show whose episodes come from TheTVDB already uses its numbering, so the
setting is not shown.

## Alternative names

XEM also knows other names for a show and its seasons. The show's are listed
as **Also known as** under its title, with their language; **+N more** shows
the rest. A season lists the names of the seasons it covers in the other
numbering: under its heading as **Season names**, and on wide screens under
it in the season list.

## Settings

Administrators choose in **Settings > Sources & dates**:

- **Find XEM through**: which of a show's remotes look it up on XEM, top
  first. The first one XEM maps is added as the show's XEM remote.
- **Other numberings**: the order the numberings are shown in, and which are
  shown. TheTVDB, scene and AniDB are shown at first; Trakt and TVRage are
  hidden.
