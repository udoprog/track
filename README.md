# track

[<img alt="github" src="https://img.shields.io/badge/github-udoprog/track-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/track)
[<img alt="crates.io" src="https://img.shields.io/crates/v/track.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/track)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-track-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/track)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/udoprog/track/ci.yml?branch=main&style=for-the-badge" height="20">](https://github.com/udoprog/track/actions?query=branch%3Amain)

A self-hosted web service for tracking show and movie progress and what to
watch next.

This is a reimagining of [ontv], which in turn was a reimagining of my old
Python-based CLI application. It now runs as a server backed by SQLite with
a web frontend written in [Yew].

Still in the experimental stage. Users beware!

[ontv]: https://github.com/udoprog/ontv
[Yew]: https://yew.rs

<br>

## Features

* A dashboard of what to watch next, upcoming releases, and a weekly
  schedule.
* A queue of what's pending to be watched.
* Detailed watch history for shows and movies.
* Show, movie, and people pages with translated titles and overviews.
* Metadata synced from [TheTVDB], [TheMovieDB], and [TVmaze], with
  configurable sync sources, languages, and release filters.
* Search for new shows and movies to track.
* Backups of your remotes and watch history as JSON lines.

[TheTVDB]: https://thetvdb.com
[TheMovieDB]: https://www.themoviedb.org
[TVmaze]: https://www.tvmaze.com

<br>

## Running track

The frontend is built with [trunk] into `dist/`. The server embeds that
directory, so it has to be built first:

```text
$ trunk build --release
$ cargo run --release
```

By default the server listens on `127.0.0.1:3000`, stores its state in
`track.db`, and caches images in `image-cache`. See `--help` for how to
change this.

Once it is running, go to `Settings` and configure your API keys for
TheTVDB and TheMovieDB. Unfortunately I cannot help you with this.

[trunk]: https://trunkrs.dev

<br>

## Development

During development you can run the server and the frontend separately. The
trunk dev server proxies `/api/` and `/ws` to the server on port `3000`:

```text
$ cargo run
$ trunk serve
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

[yew-e2e]: https://github.com/udoprog/yew-e2e

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

## Importing from ontv

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
those are what the backup covers. Passwords, sessions and login links are not
exported, so a restored user needs a new login link to sign in.

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
