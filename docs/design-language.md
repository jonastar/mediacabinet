# Design language

Nov 5, 2026 · Jonas

How Media Cabinet looks and behaves on every screen: the web UI and the players in `docs/clients.md`. Status: plan, apart from the tokens. The logo, icons and animations are in `docs/brand/`.

**The aim:** someone moving from the web UI to their TV or phone recognises the product and its words straight away, and each platform still feels at home. Identity is shared, platform conventions are not.

## Where the truth lives

| What | Source |
| --- | --- |
| Values: colour, type, spacing, durations | `design/tokens.json` |
| Rules: components, wording, focus, player behaviour | This document |
| How TV screens look and behave in practice: layouts, focus paths, timing | The Samsung and LG app in `players/web/` |

**The Samsung and LG app is the reference for TV.** It's built first and runs in a desktop browser with arrow keys, so anyone building Android TV or tvOS can open it and see how a screen should work, without needing a TV.

- **It's a reference for behaviour and layout, not a limit on looks.** Its CSS is restricted for old TVs. Android TV and tvOS match its structure, sizes and focus behaviour, and can add what their platforms do well, such as Apple TV's focus effect or backdrop blur.
- **Decisions get written down.** Swift and Kotlin developers shouldn't have to read CSS to learn a rule. When building the TV app settles something, the rule goes in this document and the value in the tokens.
- **Disagreements get resolved, not left in place.** If the app and this document differ, decide which is right and change the other.

## What's shared and what isn't

| Shared everywhere | Follows the platform |
| --- | --- |
| Colours, Manrope, shape | Navigation (tabs on phones, a side rail on TVs) |
| Poster, card, rail, hero, badge and progress bar | Keyboards, pickers and system dialogs |
| Wording for each state | AirPlay, picture in picture, system media controls |
| Launch, loading and playback animations | Focus effect on Apple TV (see Focus) |
| Player behaviour and control order | Minimum touch target (44 pt on iOS, 48 dp on Android) |

## Tokens

`design/tokens.json` is the one source for shared values. A small script generates `web/src/tokens.css`, fixed values for the Samsung and LG screens (older TVs have no CSS variables), a Swift file and a Kotlin file. The generated files are committed, and the pre-commit hook checks they're current. See `design/README.md`.

Only values used on more than one platform go in: colour, state colours, radii, spacing, type sizes, durations. `--template-*` and `--bg-sidebar` stay in `index.css`, because only the web has those screens.

### Colour

Dark only:

| Role | Value |
| --- | --- |
| Background, surface, raised surface | `#0F1115`, `#171A21`, `#1F232C` |
| Border, control border | `#262A33`, `#2E333D` |
| Text, secondary, muted | `#E8E6E1`, `#B5BAC3`, `#9AA0AA` |
| Accent, accent hover, text on accent | `#2BB5A6`, `#5FD0C3`, `#04201D` |
| Available, missing, upcoming, attention | `#4CC38A`, `#F07A5A`, `#6EA8FE`, `#F2B544` |

Badges use a 14–18% tint of the state colour with its lighter text colour. Artwork always sits under a dark scrim so text keeps 4.5:1 contrast (`lib/artwork.ts` has the gradients).

The app background (`#0F1115`) differs from the brand's dark background (`#121214`), which the animations default to. Set `--mc-bg` to the app background wherever an animation sits on a page, or its outline will show. On TVs without CSS variables, generate the animation with the right colour instead.

### Type and size

Manrope everywhere, bundled with each app (it's under the Open Font License). The Samsung and LG app uses static font files rather than the variable font, which older TVs may not handle. Titles are weight 800 with tight tracking. JetBrains Mono is only for technical details such as player stats.

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
- **Series page on TV:** hero with the episode up next as its primary action, then season tabs, then that season's episode rows, opening at the season up next. Moving focus onto a tab with the remote shows that season. A pointer has to click, because it passes over the tabs on its way to the rows.

## Wording

Players use Watch mode wording only, since Manage stays on the web. `web/src/lib/wording.ts` is the source: "Ready", "Up next", "File offline", "Not in library". Android and Apple copy those strings, and a change there includes them. Reasons from the server (why a stream is converted) come through as they are.

## Focus

Applies to all TVs. The web UI has the same need for keyboard use (Phase 9).

- **Something is always focused**, and each screen defines where focus starts: normally the primary action.
- **The focused item is obvious from across a room:** 1.08 scale and a 4 px accent ring, over 150 ms (1.02 for full-width rows, which would otherwise leave the safe area). Apple TV uses its system card effect with the accent colour wherever a control has one, because people expect it there.
- **Movement is predictable.** Left and right stay within a rail, and up and down move between rails, returning to the last item used in each.
- **Back goes back one step:** close a panel, then leave the screen and restore focus to where the person came from. At the top level, it asks before leaving the app, except on Apple TV, where the Menu button's system behaviour applies.
- **A pointer moves the same focus.** On remotes with one (LG's Magic Remote), hovering focuses a control, so there's only ever one highlight, and an arrow key carries on from there. The wheel scrolls, and rails show arrows at their ends while the pointer is in use. Hovering never opens a text field, since that can bring up the on-screen keyboard.
- **No hover-only actions.** Anything that appears on hover on the web needs a focused or selected form on TV.
- **Text fields stay in the top half of the screen,** clear of the on-screen keyboard, which covers the bottom and may not tell the page it's there.

## Player

The same on every platform, so a person never has to relearn it:

- **Controls, left to right:** play or pause, skip back and forward 10 seconds (as the web player's arrow keys do), the timeline, then tracks and versions. Next episode appears near the end.
- **Custom controls everywhere**, because of how HLS sessions work (see `docs/clients.md`), using `offset_seconds` and `duration_seconds` for the timeline.
- **Remotes:** left and right seek while the controls are hidden, and any other key shows them. Back closes an open panel, then hides the controls, then leaves the player.
- **Pointers:** moving it shows the controls, clicking the timeline seeks there, and clicking the picture puts away what's over it.
- **Failure:** try the fallback steps in `docs/clients.md` before showing an error. An error says what failed and offers a way back.

## Motion

- `launch-build-in` on app start, `loading-light-leak` while waiting, `playback-press-open` when playback starts. They can be rebuilt natively from the timings in the SVGs.
- Motion never delays anyone: if playback is ready before the reveal ends, jump to the end state.
- Follow the system's reduced motion setting where the platform has one: static mark for launch and loading, straight cut to the poster for playback.
- Older TVs may struggle with animation. If one stutters on an older model, use the static version on that engine. The focus effect is the exception: if the scale stutters, drop the scale and keep the ring, since focus must always be visible.
