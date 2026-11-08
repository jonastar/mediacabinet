# Guests

Nov 8, 2026 · Jonas

People can try a server without an account: "Continue as guest" on the login page, in the TV app and in other apps. This is for a public demo server linked from the website, and for app store review, where reviewers point an app at that server. It's off unless an admin turns it on. Status: built (`crates/core/src/guests.rs`, migration 0023).

## Turning it on

Manage → Users → Sign-in → **Let people continue as a guest**, or `CABINET_GUEST_ACCESS=true`. The env var wins, and makes the switch read-only. Guests can't get in until the server has been set up, so there's always an admin who can turn it off again.

Turning it off signs out every guest and removes them, from the switch or by restarting without it.

## What a guest is

Each "Continue as guest" makes a new account for that one visitor, named `guest-` and six hex digits and shown as "Guest". A shared guest account would show strangers each other's resume points, watched marks and playlists, and on a public server, playlist names anyone could type.

A guest:

- watches, keeps resume points and watched marks, picks a series' ordering, and makes playlists, all private to them, as for any viewer;
- is always incognito, so other users and guests never see what they watched (admins still do);
- can approve a TV's pairing code at `/pair`, which signs the TV in as the same guest;
- can't change their name, email, password or privacy setting, or link another account (`PATCH /api/me`, `/api/me/logins` and linking are refused);
- has no password, so nobody can sign in as them again once their sessions end;
- isn't listed in Manage → Users. The Sign-in section shows how many there are.

Signing in as a guest isn't logged in activity: on a public server it would bury everything else.

## When guests go

A guest is removed:

- when they sign out of their last session (signing out warns that their history goes with it);
- once all their sessions have expired, swept when the next guest arrives and at startup (sessions last 30 days from last use, a device's a year);
- when the server has 1,000 guests (`MAX_GUESTS`) and another arrives: the guest seen least recently makes room.

## Ways in

| Where | How |
| --- | --- |
| Web UI | "Continue as guest" on the login page. A link to `/?guest` goes straight in, for a website's "Try the demo" button. |
| TV app | "Continue as guest" beside the pairing code, when `GET /api/server-info` says `guest_access`. |
| Other apps | `GET /api/server-info` → `guest_access: true` means offer it; `POST /api/auth/device/guest` with `{ "device_name" }` returns a device token, as `POST /api/auth/device` does. |
| Browser API | `POST /api/auth/guest` sets the session cookie and returns the user. |

`GET /api/auth/status` also reports `guest_access`, and `User` has `is_guest`.

## Running the demo server

- Openly licensed films and shows only (Big Buck Bunny, Sintel, Tears of Steel, Elephants Dream, Cosmos Laundromat…).
- Keep the admin account's password private. Turn on incognito for the admin, so guests don't see what the admin watched.
- Store review notes can say "choose Continue as guest" instead of giving out a password. If a store insists on a username and password, create a normal non-admin account for it as well.
- Transcoding is the expensive part. Encode the films so they direct play on common devices, and keep the server's transcoding within what the machine can take. Guests can't change libraries or settings, but they can start streams.

## Not built yet

- Limits per address on how many guests can be made, or how many streams one guest can run.
- Starting new guests with some watch history, so Up next and Continue watching aren't empty on a first visit.
