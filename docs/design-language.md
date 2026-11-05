# Design language

Nov 5, 2026 · Jonas

How Media Cabinet looks and behaves on every screen: the web UI and the players in `docs/clients.md`. Status: plan. `index.css` holds today's values. The logo, icons and animations are in `docs/brand/`.

**The aim:** someone moving from the web UI to their TV or phone recognises the product and its words straight away, and each platform still feels at home. Identity is shared, platform conventions are not.

## What's shared and what isn't

| Shared everywhere | Follows the platform |
| --- | --- |
| Colours, Manrope, shape | Navigation (tabs on phones, a side rail on TVs) |
| Poster, card, rail, hero, badge and progress bar | Keyboards, pickers and system dialogs |
| Wording for each state | AirPlay, picture in picture, system media controls |
| Launch, loading and playback animations | Focus effect on Apple TV (see Focus) |
| Player behaviour and control order | Minimum touch target (44 pt on iOS, 48 dp on Android) |

## Tokens

`design/tokens.json` becomes the one source for shared values. A small script generates `web/src/tokens.css`, a Swift file and a Kotlin file. The generated files are committed, and CI checks they're current.

Only values used on more than one platform go in: colour, state colours, radii, spacing, type sizes, durations. `--template-*` and `--bg-sidebar` stay in `index.css`, because only the web has those screens.

### Colour

Dark only, using today's values from `index.css`:

| Role | Value |
| --- | --- |
| Background, surface, raised surface | `#0F1115`, `#171A21`, `#1F232C` |
| Border, control border | `#262A33`, `#2E333D` |
| Text, secondary, muted | `#E8E6E1`, `#B5BAC3`, `#9AA0AA` |
| Accent, accent hover, text on accent | `#2BB5A6`, `#5FD0C3`, `#04201D` |
| Available, missing, upcoming, attention | `#4CC38A`, `#F07A5A`, `#6EA8FE`, `#F2B544` |

Badges use a 14–18% tint of the state colour with its lighter text colour. Artwork always sits under a dark scrim so text keeps 4.5:1 contrast (`lib/artwork.ts` has the gradients).

The app background (`#0F1115`) differs from the brand's dark background (`#121214`), which the animations default to. Set `--mc-bg` to the app background wherever an animation sits on a page, or its outline will show.

### Type and size

Manrope everywhere, bundled with each app (it's under the Open Font License). Titles are weight 800 with tight tracking. JetBrains Mono is only for technical details such as player stats.

Sizes are per form factor. TV values are at 1920×1080, which is one-to-one on tvOS and in Samsung and LG apps at that resolution. **Android TV lays out at 960×540 dp, so halve them there.**

| Role | Phone | Web | TV |
| --- | --- | --- | --- |
| Hero title | 34 | 76 | 76 |
| Page title | 28 | 30 | 57 |
| Section title | 20 | 22–24 | 38 |
| Body | 15–16 | 15–16 | 29 |
| Meta | 13 | 13–13.5 | 25 |
| Smallest text | 12 | 12 | 23 |

The phone and TV columns are starting points, to be judged on real screens. For TV, that means from across a room.

### Shape and spacing

- Radius: 10 for controls, 14 for cards (20 on TV), pill for primary buttons.
- Spacing in steps of 4; TV layouts mostly use 16, 24, 32 and 48.
- TV safe area: keep content 96 from the sides and 60 from the top and bottom. Backdrops run to the edge.
- TV cards: posters about 240×360, landscape cards about 400×225.

## Components

The same parts, named the same in every codebase:

- **Poster:** 2:3, title below it, optional badge.
- **Landscape card:** 16:9, with a progress bar when started. Used for keep watching and episodes.
- **Rail:** a titled row that scrolls sideways.
- **Hero:** backdrop, scrim, title, one meta line, and the primary action first. Resume when there's something to resume, Play otherwise.
- **State badge:** label and colour from the state table.
- **Progress bar:** accent on a dim track, the same on every card and in the player.
- **Episode row:** thumbnail, number, title, meta, badge. The number follows the viewer's ordering (`S01E03`, `03` or `E03`).

## Wording

Players use Watch mode wording only, since Manage stays on the web. `web/src/lib/wording.ts` is the source: "Ready", "Up next", "File offline", "Not in library". Android and Apple copy those strings, and a change there includes them. Reasons from the server (why a stream is converted) come through as they are.

## Focus

Applies to all TVs. The web UI has the same need for keyboard use (Phase 9).

- **Something is always focused**, and each screen defines where focus starts: normally the primary action.
- **The focused item is obvious from across a room:** 1.08 scale and a 4 px accent ring, over 150 ms. Apple TV uses its system card effect with the accent colour wherever a control has one, because people expect it there.
- **Movement is predictable.** Left and right stay within a rail, and up and down move between rails, returning to the last item used in each.
- **Back goes back one step:** close a panel, then leave the screen and restore focus to where the person came from. At the top level, it asks before leaving the app, except on Apple TV, where the Menu button's system behaviour applies.
- **No hover-only actions.** Anything that appears on hover on the web needs a focused or selected form on TV.

## Player

The same on every platform, so a person never has to relearn it:

- **Controls, left to right:** play or pause, skip back and forward 10 seconds (as the web player's arrow keys do), the timeline, then tracks and versions. Next episode appears near the end.
- **Custom controls everywhere**, because of how HLS sessions work (see `docs/clients.md`), using `offset_seconds` and `duration_seconds` for the timeline.
- **Remotes:** left and right seek while the controls are hidden, and any other key shows them. Back closes an open panel, then hides the controls, then leaves the player.
- **Failure:** try the fallback steps in `docs/clients.md` before showing an error. An error says what failed and offers a way back.

## Motion

- `launch-build-in` on app start, `loading-light-leak` while waiting, `playback-press-open` when playback starts. They can be rebuilt natively from the timings in the SVGs.
- Motion never delays anyone: if playback is ready before the reveal ends, jump to the end state.
- Follow the system's reduced motion setting where the platform has one: static mark for launch and loading, straight cut to the poster for playback.
- Older TVs may struggle with animation, so if an animation stutters on a 2021 model, use the static version there.
