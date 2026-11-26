# track

[<img alt="github" src="https://img.shields.io/badge/github-udoprog/track-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/track)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-track-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/track)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/udoprog/track/ci.yml?branch=main&style=for-the-badge" height="20">](https://github.com/udoprog/track/actions?query=branch%3Amain)

**Know what to watch tonight.** track is a self-hosted tracker for the shows
and movies you follow. It keeps your watch history, tells you what's next,
and shows you what's airing this week, all from one small server you run
yourself.

No ads, no recommendations engine, no account with someone else's service.
Just your library, your history, and a fast web app that works as well on
your phone as on your desktop.

* **One binary.** The server, the web app and a SQLite database. Build it,
  run it, open a browser.
* **Your data stays yours.** Everything lives in a single database file, and
  the parts that can't be synced back can be exported as plain JSON lines.
* **Built for the whole household.** Every user has their own library,
  queue and history, and an admin hands out sign-in links.

track is the latest incarnation of [ontv], which in turn grew out of an old
Python CLI of mine. It is written in Rust from end to end, with a web
frontend in [Yew]. It is still young, so expect rough edges and keep backups.

[ontv]: https://github.com/udoprog/ontv
[Yew]: https://yew.rs

<br>

## What it does

<br>

### Know what's next

* **What's Next** puts the next episode of every show you're watching up
  front, poster first, with one button to mark it watched.
* **Upcoming** and a weekly **Schedule** show what's about to air, so a new
  season never sneaks past you.
* A **Queue** lays out everything pending as a single timeline, with what's
  airing now pinned at the top.
* **History** lists every watch, newest first. Filter the whole dashboard by
  text to find a title in any tab.

<br>

### Keep an honest history

* Mark an episode or movie watched now, at the time it aired, or at any time
  you pick. Mark the rest of a season in one go.
* Watches that lose their episode after a resync can be reassigned rather
  than lost.
* Import years of history from [ontv] in one command.

<br>

### Metadata that gets it right

* Shows and movies are synced from [TheTVDB], [TheMovieDB] and [TVmaze].
  Attach several sources to a title and choose which one leads.
* Episode numbering from [XEM], [AniDB] and scene releases, with an editor
  to map one numbering onto another. Anime fans, this one is for you.
* Titles and overviews in the language you prefer, with release filters per
  country.
* Pages for every show, movie and person, with cast, "Known for", and image
  galleries where you choose the posters and backdrops.

<br>

### Pleasant to use

* Live updates over a websocket: mark something watched on your phone and
  your desktop already knows.
* Phones are first class. Every page is designed for a narrow screen too,
  with touch-sized targets.
* Light, dark or follow-the-system themes.
* A searchable help panel, with help marks next to anything that needs
  explaining.

<br>

### Easy to run

* Multiple users, with an admin page, registration links, login throttling
  and optional [Cloudflare Access] sign-in.
* Backups as newline-delimited JSON you can read, diff and annotate.
* A [kick] deploy profile that installs track as a systemd service over ssh.

[TheTVDB]: https://thetvdb.com
[TheMovieDB]: https://www.themoviedb.org
[TVmaze]: https://www.tvmaze.com
[XEM]: https://thexem.info
[AniDB]: https://anidb.net
[Cloudflare Access]: https://www.cloudflare.com/zero-trust/products/access/

<br>

## Getting started

The frontend is built with [trug] into `dist/`. The server embeds that
directory, so it has to be built first:

```text
$ trug build --release
$ cargo run --release
```

That's it. Open <http://127.0.0.1:3000>.

On first start, with no users, the server logs a registration link for the
administrator `root`. Open it to choose a password and sign in.
`--bootstrap-admin` names the administrator by another login, or by an
email.

Then go to `Settings` and add your API keys for TheTVDB and TheMovieDB.
Unfortunately I cannot get those for you. Then search for something you're
watching, track it, and you're off.

By default the server listens on `127.0.0.1:3000`, stores its state in
`track.db`, and caches images in `image-cache`. See `--help` for how to
change this.

[trug]: https://github.com/udoprog/trug

<br>

## Coming from ontv

If you have been using [ontv], its YAML database can be imported like this:

```text
$ cargo run --bin import -- --source ~/.config/ontv --db track.db
```

Run it once, into a fresh database; it is not safe to rerun. Shows and movies
are only matched by remote id, so a second run can fail or duplicate entries,
and the settings from `config.yaml` (API keys, theme, dashboard page) replace
the current configuration and preferences on every run. If an import is
aborted, start over with a fresh database.

This will take a while, so go get a ☕.

<br>

## Backing up your data

Most of the data in the database can be recovered by syncing it again from
the remotes. What can't be recovered is the users, the remotes of each show and
movie, what each user tracks, their preferences and their watch history, so
those are what the backup covers. Passwords, sessions and registration links
are not exported, so a restored user needs a new registration link to sign in.

```text
$ track export --output backup.jsonl
$ track import --input backup.jsonl
```

A backup is newline-delimited JSON. Lines starting with `#` are comments, so
you can annotate a backup by hand. Importing is idempotent, so entries which
already exist are skipped.

Export only reads: it refuses a database that does not exist or has pending
migrations, and reads everything from one snapshot, so it can run while the
server is up. Stop the server before importing: import writes without
journaling, so it must not run alongside the server and is not crash-safe.

<br>

## Issuing a sign-in link

When a user (such as a restored one, or the only admin) has no working link
and no admin can issue one, issue it from the command line. It prints a reset
link for an active user and a registration link otherwise, using up their
earlier links:

```text
$ track issue-link root --public-url https://track.example.com
https://track.example.com/register/...
```

Without `--public-url` it prints the link's path. An unknown login or email
fails.

<br>

## Deploying

The `remote` profile in `Kick.toml` deploys track over ssh with [kick] as
`integration`, and leaves the host to the command line. It builds the frontend,
builds the server with the frontend bundled into it, installs it as
`/usr/local/bin/track`, and restarts the `track` service:

```text
$ kick deploy --to remote --host moore
```

Pass `--host` more than once to deploy to several hosts, and `--dry-run` to
print the unit and every command without running them:

```text
$ kick deploy --to remote --host moore --host otherhost
$ kick deploy --to remote --host moore --dry-run
```

Kick also manages `/etc/systemd/system/track.service`, which it renders from
`[deploy.systemd]` and replaces whenever the installed unit differs. Edit the
unit there, not on the host. The deploying user needs passwordless `sudo` on the
host, since kick runs its remote commands with `sudo -n`.

[kick]: https://github.com/udoprog/kick

<br>

## Hacking on track

During development you can run the server and the frontend separately. The
trug dev server proxies `/api/` and `/ws` to the server on port `3000`:

```text
$ cargo run
$ trug serve
```

To preview loading states as they would look on a slow connection, you can
inject an artificial delay in milliseconds into every websocket request:

```text
$ cargo run -- --delay 200..800
```

The browser tests in `crates/e2e` use [yew-e2e]. They build the frontend and
the server, and give every test its own server with a fresh database, so they
never touch `track.db`. They need Firefox with `geckodriver`, or Chrome:

```text
$ cargo test -p e2e
$ cargo test -p e2e -- settings::
$ cargo test -p e2e -- --headed --last-session
```

How the interface looks and behaves is written down in [DESIGN.md].

[yew-e2e]: https://github.com/udoprog/yew-e2e
[DESIGN.md]: https://github.com/udoprog/track/blob/main/DESIGN.md
