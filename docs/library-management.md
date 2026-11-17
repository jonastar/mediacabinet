# Library management

How Media Cabinet writes to library folders: what a root allows, how existing files get organised, and how new files come in. It replaces the per-root write policy (`managed`, `adaptive`, `read_only`) and the import-target flag as people see them.

## Principles

- **Nothing changes on disk by itself.** A scan only reads. Files are renamed, moved or added only when someone runs Organise or an import, or drops a file into a library's inbox. Download clients, other media servers and \*arr tools all depend on paths, and a wrong match that renames a whole series quietly is worse than a messy folder.
- **Every change can be previewed.** Organise and the import tool show the before and after, and you can untick what you don't want before applying it. The API keeps `dry_run`.
- **Never overwrite** (unchanged from today). If a destination exists, that file is skipped and the plan says why.

## Roots: writable or read-only

A root has one setting, **Read-only**, off by default.

- **Writable** (the default) means Cabinet may write to the root when asked: Organise, imports, moving a series, fixing a folder's IDs. Being writable does nothing on its own, so it's a safe default.
- **Read-only** means Cabinet never writes there at all. Organise skips the root's files, imports go to another root, and sidecars Cabinet makes go to the cache folder. This is for a NAS share, a folder another tool owns, or a download client's seeding folder.

Read-only has no override. To fix a folder's IDs on a read-only root, turn read-only off first. That replaces today's "Rename anyway…" and its `force` flag.

Folder libraries keep their rule: their roots are always read-only (see `folder-libraries.md`).

## Where new items go

Each movie or TV library has a **"New items go to"** setting: a dropdown of its writable roots. It replaces "Make import target" on a root, and since it only lists writable roots, the old "a read-only root can't be the import target" rule can't come up.

An imported file goes to:

1. its series' stored folder (`series_location`), if that root is writable;
2. for a movie, the root of one of its present files, if that root is writable;
3. otherwise the library's new-items root. A series then gets a second location there, as it does today.

## Organise

A manual tool that brings files in line with the root's folder layout. It replaces the `managed` policy: any writable root can be organised, and nothing is organised until you run it.

- **Scope:** run it on a library, a root, or a single series or movie. The series and movie forms are the existing Organise action on the Manage panel.
- **Preview:** a before → after list, grouped by series or movie, with a checkbox on each change. Files already in place aren't listed. Skipped files say why: read-only root, destination taken, or the layout can't name the file (for example, an episode number is missing).
- **Apply:** the page sends the ticked moves in batches of 100 (`POST /api/organize/apply`) and shows how far it is. It isn't a task: tasks recompute their work when they run and carry no choices, and Organise only renames within a root, which is quick. Each move is checked against its file as it is now (still at `from`, on a writable root, `to` inside the root and free), so a stale preview fails file by file rather than moving the wrong thing. File rows are updated in place, so links and watch history follow the files.
- **Also lives here:** moving a series to another root and fixing a folder's provider IDs, so the tool offers every disk change for a title in one place.

## Import

Two ways in, both matching the file, placing it as described above and linking it.

### The import tool (by hand)

Pick a folder the server can read, outside every library folder (`POST /api/imports/list`). Each video is matched against the series and movies already in the library, by title and episode numbers; anything not yet in the library has to be added first. Check or correct the match for each video, and choose:

- **Copy, hardlink or move.** The default is **copy**, so the original is never touched (an import's source is always outside the library's folders). Hardlink is offered when the files are on the same filesystem as a writable folder of the library ("no extra space; the download client keeps seeding"); elsewhere it falls back to a copy. Move is always available.
- **Naming:** **Rename to the layout** (default) or **Keep original filenames**. Keeping names is today's `adaptive` behaviour: a file goes next to the existing files of its season if there are any, or into the series' folder made by the layout.

Then **Preview** runs each chosen file as a dry run and shows where it goes; **Import** does it, one file at a time (`POST /api/imports/file`).

`POST /imports` keeps its current default of `hardlink` for API clients. The copy default above is chosen by the web UI, which always sends the mode.

### The inbox (dropped files)

A movie or TV library can have an **inbox folder**. Cabinet owns it: anything that lands there is imported into the library by the layout. The usual setup is pointing a download client's "move completed downloads to" folder at the inbox.

- The inbox must not be inside one of the library's roots, so scans never see it as library content.
- **Transfer:** hardlink when the inbox and the destination are on the same filesystem, so a client that seeds from the inbox keeps working; move when they aren't. An imported file left behind in the inbox is remembered by device and inode, so it's never imported twice and disappears from the list once the client deletes it.
- **What it imports alone:** a video it recognises as surely as a scan must to link a file without asking (the matcher's accept threshold), for episodes or a movie that have no file yet. Anything else stays in the inbox as **needs attention**, with the reason, on the library's page: not recognised, unsure, no episode in the name, already has a file (an upgrade is a decision for a person), or failed the probe. "Import by hand" opens the import tool on the inbox.
- **Watching:** every minute, the server looks for finished videos it hasn't seen yet, and only then starts an `inbox` task, so a quiet inbox leaves no task history. "Look again now" runs one by hand, which also looks again at what needed attention, in case the library has changed since. Filesystem notifications aren't used yet; they're unreliable on network shares anyway.

### Is it finished downloading?

There's no single convention for marking an unfinished file, and some clients (Deluge, and qBittorrent by default) give the file its final name and full size from the start. So the inbox, and the import tool when it lists a folder, use three checks:

1. **Skip known markers.** Suffixes `.part`, `.partial`, `.!qB`, `.!ut`, `.!bt`, `.crdownload`, `.tmp`; hidden files, which also covers rsync's `.name.XXXXXX`; anything with an `.aria2` file next to it; and folders named `_UNPACK_*` or `_FAILED_*` (SABnzbd, NZBGet).
2. **Settle time.** A file is taken only once its modification time is a while ago (2 minutes by default, `CABINET_IMPORT_SETTLE_SECONDS`). Pre-sized files still change their mtime as data is written, so this catches most of them; a stalled download can still get through.
3. **Probe before filing.** The inbox runs ffprobe first: a file shorter than half the matched items' runtime, or one ffprobe can't read, is held as **needs attention** rather than filed. This catches a partial file that slipped past the first two checks. Without ffprobe on the server this check is skipped.

The setup instructions recommend letting the client hand files over when they're complete, since then everything in the inbox is finished. Asking download clients' APIs directly, as Sonarr and Radarr do, can come later.

## Changing over

Stored data mostly stays as it is, which keeps the API additive (`releasing.md`):

- **Read-only** is `write_policy = 'read_only'`. `managed` and `adaptive` both mean writable. The API keeps accepting and returning `write_policy`. A new `read_only` boolean is added beside it, and once the old field is no longer needed, clients use only the boolean.
- **The managed/adaptive difference** moves from the root to the import request (Naming). An API import that doesn't say how to name files keeps following its destination root's stored policy, so existing integrations behave the same.
- **Organise** stops checking for `managed`: it runs on any writable root.
- **"New items go to"** is stored as today's `is_import_target`, with its one-per-library index. Only the UI changes.
- **New:** migration `0024_inbox.sql` adds the library's `inbox_path` and `inbox_file`, the videos an inbox has dealt with (path, device, inode, size, mtime, imported or needing attention, and why).
- **Fixing a folder's IDs** no longer takes `force`: the field is still accepted and ignored, and read-only roots are refused.

## Not decided yet

- Whether a library can have more than one inbox, for example one per download client.
- Whether the inbox should also take subtitles and other sidecars dropped on their own, or only those that come with a video.
