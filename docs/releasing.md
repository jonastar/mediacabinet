# Versions and releases

Each app has its own version and releases on its own:

| App | Version in | Changelog |
| --- | --- | --- |
| `server`, with the web UI | `Cargo.toml` (every crate), `web/package.json` | `CHANGELOG.md` |
| `android`, phone and TV | `players/android/gradle.properties` | `players/android/CHANGELOG.md` |
| `apple`, iOS and tvOS | `players/apple/Version.xcconfig` | `players/apple/CHANGELOG.md` |
| `tizen` | `players/tizen/VERSION`, `config.xml` | `players/tizen/CHANGELOG.md` |
| `webos` | `players/webos/package.json`, `appinfo.json` | `players/webos/CHANGELOG.md` |

The server and web UI ship together and are never mixed across versions. The TV
app in `players/web` has no version of its own: it ships inside `tizen`, `webos`
and the server (at `/tv-mode/`), so a change to it goes in the changelog of each
app it changes. Phone and TV, and iOS and tvOS, share a version because they're
one build each.

## Version numbers

Versions are `YEAR.N.PATCH`: the year, the release that year, and the patch.
`2026.1.0`, `2026.2.0`, then `2027.1.0` for the first release in 2027, with
fixes to a release as `2026.2.1`. The full year says what it is at a glance.

Every commit that isn't a release carries the version it's working toward with
`-dev` (`2026.3.0-dev`), so a build always says what it is (`GET /api/health`,
`GET /api/server-info` for the server). That's a best guess: if the release
happens in a new year, it becomes `2027.1.0` instead.

Some stores and manifests only take numbers, and `scripts/set-version.sh` writes
them for you:

- **Android:** `versionCode` is worked out from the version in
  `players/android/phone/build.gradle.kts`: `year*100000 + N*1000 + patch*10`,
  plus 1 for the TV app, because both share one Play listing and every upload
  needs a higher code. That leaves room for 99 releases a year and 99 patches.
- **Apple:** `MARKETING_VERSION` has no `-dev`. The Xcode project should take
  both from `Version.xcconfig` once it exists.
- **webOS:** `appinfo.json` has no `-dev`.
- **Tizen:** `config.xml` takes `{0-255}.{0-255}.{0-65535}`, so it gets a
  two-digit year (`26.1.0`). That's what Samsung's seller portal shows; the app
  itself says `2026.1.0`.

## Tags and branches

- Tags: `<app>/v<version>`, such as `server/v2026.2.0` and `android/v2026.1.0`.
  `git describe --match 'android/v*'` finds an app's last release.
- `main` works toward each app's next release.
- Patch releases come from `release/<app>/<year>.<N>`, such as
  `release/server/2026.2`, made from that release's tag when the first fix is
  needed.

## Releasing

`scripts/release.sh <app>` does it all, without pushing:

1. Sets the release version, which is this year's next release (from the tags)
   on `main` or the patch being worked on on a release branch, and replaces the
   changelog's "(unreleased)" heading with it and the date.
2. Commits "Release <app> <version>" and tags it `<app>/v<version>`.
3. Sets the next `-dev` version, adds its "(unreleased)" heading to the
   changelog and commits "Start <app> <version>".

Then push the branch and the tag. `scripts/set-version.sh <app>` shows an app's
version, and `scripts/set-version.sh <app> <version>` sets it everywhere.

## Patch releases

1. Fix on `main` first.
2. The first time a release needs a fix,
   `scripts/release.sh <app> --patch 2026.2` makes `release/<app>/2026.2` from
   `<app>/v2026.2.0` and starts `2026.2.1-dev` on it.
3. Bring the fix over with `git cherry-pick -x <fix>`, with its changelog line
   under the branch's "(unreleased)" heading.
4. `scripts/release.sh <app>` on the branch releases `2026.2.1` and starts
   `2026.2.2-dev`.
5. Copy the `2026.2.1` changelog section to `main` too, below the section being
   worked on, so `main`'s changelog has every release.

## Changelogs

Each app's changelog gets an entry in the same commit as each user-visible
change, under its top section (the version being worked toward). Internal
refactors and tests don't need one. Keep entries to one line about what changed
for the people using it.

## Apps and servers together

Apps stay in use for months after an update, and servers go without updates for
months, so each side states the oldest of the other it works with. Neither can
know the newest, so neither states it.

- **Each app's oldest server:** `MIN_SERVER_VERSION` in
  `players/android/core/.../Labels.kt` and `players/web/src/version.ts` (for
  Tizen and webOS), checked against `version` from `GET /api/server-info` on
  the connect screen. Raise it when an app starts using something a newer
  server added. A `-dev` server counts as the version it's working toward.
- **The server's oldest of each app:** `MIN_APP_VERSIONS` in
  `crates/server/src/api.rs`, reported as `min_app_versions` in
  `GET /api/server-info`, which tells older apps to ask for an update. (Its
  `min_app_version` is from before apps had their own versions and stays
  `0.0.0`.)

So the API only grows:

- Adding endpoints, optional request fields and response fields is always fine.
  Apps ignore what they don't know.
- Removing or changing anything an app uses waits until no supported version of
  any app needs it. Raise those apps' entries in `MIN_APP_VERSIONS`, and give
  the removal its own changelog line.
- `GET /api/server-info` is read before an app knows the server's version, so
  `version` in it never changes or goes away.
