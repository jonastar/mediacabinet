# Metadata storage

Nov 7, 2026 · Jonas

How much disk metadata takes, and the settings planned for people short of it. Status: plan. Today everything downloads at high quality and thumbnails are always on; the roadmap's Phase 8 tracks the settings.

**The aim:** the defaults suit the usual server, which has 100 GB or more to spare: high quality, thumbnails on. Tight setups can turn things down, but nobody should have to think about this to get a good library.

## What it takes today

For a library of 10,000 episodes, roughly 2 GB at most:

| What | Size | Basis |
| --- | --- | --- |
| Episode stills, `w780` | About 800 MB | Measured: 75–94 KB each on a small TMDB sample, against 14–21 KB at `w300` |
| Generated thumbnails, 640 wide | A few hundred MB at most | Estimate: 20–40 KB each, and only for episodes with no still |
| Posters and backdrops | A few hundred MB | Estimate, depending on how many shows the episodes are spread over |

Stills download for every episode in the metadata, including ones with no file. Trickplay will outgrow all of this once it exists, since it stores many frames per video.

## The settings

Server-wide and admin-only, in the Manage menu.

| Setting | Values | Default |
| --- | --- | --- |
| Artwork quality | High, low, off | High |
| Generated thumbnails | On, off | On |

- **High** is today's sizes: stills `w780`, backdrops `w1280`, posters `w500`.
- **Low** is smaller versions: stills `w300`, backdrops `w780`, posters `w342`.
- **Off** stops the background download, and an image someone views is fetched without being kept. Otherwise turning artwork off would leave blank posters everywhere, when the person only wanted to save disk. This means artwork stops working when the provider can't be reached.
- **Thumbnails off** stops the `thumbnails` task asking for frames, so episodes and movies with no provider image show a placeholder.
- Trickplay gets its own switch when it's built.

Quality is a single choice rather than a size per image type, which would ask people to know TMDB's sizes.

## What has to exist first

- **A settings store.** The only one today is `auth_setting`, for sign-in. This needs a general table, an admin endpoint and a Manage section. Other options can use it later, such as how many heavy tasks run at once.
- **Removing unused images.** Files are kept by size (`artwork/{provider}/{size}/`), so lowering the quality downloads the smaller ones and leaves the larger ones on disk. Without a cleanup, changing the setting uses more space, not less. `docs/tasks.md` lists this as still to do.

## Open questions

- Does turning a setting off delete what's already stored straight away, or only stop adding to it? Deleting frees the space, but turning it back on means downloading or generating everything again.
- Should the setting show what each option would save? That's useful for deciding, but it needs a size total the server doesn't keep today.
