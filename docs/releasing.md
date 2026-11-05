# Versions and releases

Versions are `MAJOR.MINOR.PATCH`. Every commit that isn't a release carries the
version it is working toward with a `-dev` suffix, so a build always says what it
is (`GET /api/health` reports it).

- `main` works toward the next major version (`2.0.0-dev` after `1.0.0`).
- A release is a commit on its own that drops `-dev`, tagged `vX.Y.Z`. The next
  commit bumps to the following version with `-dev` again.
- Patch releases come from a `release/X.Y` branch made from the release tag when
  the first fix is needed.

`scripts/set-version.sh VERSION` sets the version in `Cargo.toml` (shared by
every crate), `Cargo.lock` and `web/package.json`.

## Changelog

`CHANGELOG.md` gets an entry in the same commit as each user-visible change,
under the top section (the version being worked toward). Internal refactors and
tests don't need one. Keep entries to one line about what changed for the user.

## Releasing from main

1. `scripts/set-version.sh 1.0.0`; in `CHANGELOG.md` replace "(unreleased)" with
   the date (`## 1.0.0 (2026-11-30)`).
2. Commit "Release 1.0.0" and tag it: `git tag -a v1.0.0 -m "Media Cabinet 1.0.0"`.
3. `scripts/set-version.sh 2.0.0-dev`; add `## 2.0.0 (unreleased)` to the
   changelog; commit "Start 2.0.0-dev".

## Patch releases

1. Fix on `main` first, then bring it to the branch: the first time,
   `git switch -c release/1.0 v1.0.0` and `scripts/set-version.sh 1.0.1-dev`
   (commit "Start 1.0.1-dev"); then `git cherry-pick -x <fix>`.
2. Release as above on the branch: `1.0.1`, tag `v1.0.1`, then `1.0.2-dev`.
3. Copy the `1.0.1` changelog section to `main` too, below the section being
   worked on, so `main`'s changelog has every release.

## API changes

Apps stay in use for months after an update, so the API only grows:

- Adding endpoints, optional request fields and response fields is always fine. Apps ignore what they don't know.
- Removing or changing anything an app uses waits until no supported app needs it. Raise `MIN_APP_VERSION` in `crates/server/src/api.rs`, which `GET /api/server-info` reports and which tells older apps to ask for an update, and give the removal its own changelog line.
