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
the first episode of each names that season.

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

The range editor lists the show's TMDB episodes beside those of every
numbering XEM has for it, a column each, with bands showing where each range
takes its episodes in all of them. A range maps onto one numbering; XEM
carries it on to the rest. Episodes no range covers are hatched, episodes
XEM doesn't know are dashed in amber, and ranges that overlap turn red and
can't be saved. In automatic mode it shows what automatic numbering does;
**Suggest from episode order** pairs the episodes one to one and shows the
result before you save it. When the numberings put the episodes in
different orders, it first asks which one to follow.

To edit, click an unmapped episode in any column to start a range there, or
a mapped one to select its range. With a range selected, click two TMDB
episodes to set its first and last, or two episodes of another numbering to
map it onto them; one click there moves where it starts. The selected
range's numbers can also be typed.

A show whose episodes come from TheTVDB already uses its numbering, so the
setting is not shown.

## Alternative names

XEM also knows other names for a show and its seasons. When it has some,
the show's ![Names](button:tag) **Names** action lists them: the show's own,
with their language, then each season's, grouped by the season of the other
numbering they name, following the show's episode numbering.

## Settings

Administrators choose in **Settings > Sources & dates**:

- **Find XEM through**: which of a show's remotes look it up on XEM, top
  first. The first one XEM maps is added as the show's XEM remote, which
  links to the show's XEM page once syncing finds it.
- **Other numberings**: the order the numberings are shown in, and which are
  shown. TheTVDB, scene and AniDB are shown at first; Trakt and TVRage are
  hidden.
