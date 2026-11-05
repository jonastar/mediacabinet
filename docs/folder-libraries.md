# Folder libraries

A library of kind `folders` is for videos no metadata provider knows about: home videos, clips, recorded talks. You browse the folders as they are on disk and play what's in them.

## How it works

- **One item per video.** Each file gets its own item, named after the file without its extension. Nothing is parsed: `trailer.mkv` is a video like any other, and `The.Matrix.1999.mkv` keeps that name. Nothing is searched for or queued for review, and no provider needs to be configured (`matcher/folders.rs`).
- **The item is a `movie` with no provider.** That's what makes playback, resume, watch state, editing and search work without their own code paths. It's also why the places that list movies must say which they mean: `list_movies` without a library leaves folder libraries out, so `/movies` and Home show only real movies.
- **The tree is derived.** Folders aren't stored; `GET /api/libraries/{id}/browse?path=` reads them from file paths (`catalog/folders.rs`). A folder with the same path on two roots shows as one.
- **Files stay the source of truth.** Each scan updates an item's title and runtime from its file, unless those fields are locked (edit a title with the lock on to keep it). A moved or renamed file keeps its item and watch history, as the scanner recognises it. A deleted file's item stays, so its history survives if the file returns: Watch mode hides it, Manage mode shows it as missing.

## What doesn't apply

- **Roots are always read-only, with no import target.** The API refuses anything else. Nothing is imported, renamed or moved.
- **No layout.** A root has to store a template, so folder libraries use the fixed `{video}`, and any other is refused.
- **No metadata refresh.** The API refuses it and the UI hides it.
- **No activity entries.** A first scan can add thousands of unrelated clips, so `items_added` skips these libraries. `item.added` and `link.created` events still go out to webhooks.
- **No monitoring.** Videos start unmonitored and the UI offers no way to change that, so they don't show up as wanted.

## Not built yet

- Local artwork: images next to a video are already scanned as sidecars, but not served. Rows show a frame from the video instead (see Thumbnails in `tasks.md`).
- Playing the next video in the folder automatically.
- A keep-watching row on Home: Home only has one for series, so videos resume from where you left them but aren't surfaced there. The same goes for movies.
