# Media Cabinet — brand assets

## Colours

| Role | Hex |
|---|---|
| Cabinet (square) | `#0F7A71` |
| Play symbol / accent | `#2BB5A6` |
| Dark background | `#121214` |
| Light background / cream text | `#F2EEE6` |
| Dark text on light | `#141416` |

The seam and the play symbol's thick outline are **real cut-outs** (transparent), so the background shows through on any surface.

## Typeface

Wordmark: **Manrope** (Medium 500 for "media", ExtraBold 800 for "cabinet"), tracking −2 %. In the lockup SVGs the text is converted to outlines, so no font is needed to display them. Manrope is licensed under the SIL Open Font License.

## Files

```
mark/      media-cabinet-mark.svg          main mark, 64×64 viewBox with padding
           media-cabinet-mark-tight.svg    cropped to the shape
           media-cabinet-mark-mono-dark.svg / -mono-light.svg   single-colour versions
           media-cabinet-mark-512.png / -1024.png               transparent PNGs
lockup/    media-cabinet-lockup-on-dark.svg/.png
           media-cabinet-lockup-on-light.svg/.png
icons/     favicon.svg, favicon.ico (16/32/48), favicon-16/32/48.png
           (the 16 px version has slightly wider cut-outs so it survives at that size)
           apple-touch-icon-180.png, app-icon-1024.png   (full-bleed dark background)
           android-maskable-192.png / -512.png           (mark inside the maskable safe zone)
motion/    see below
preview.html   opens everything in a browser, with replay buttons for the animations
```

Favicon snippet:

```html
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="icon" href="/favicon.ico" sizes="48x48">
<link rel="apple-touch-icon" href="/apple-touch-icon-180.png">
```

## Motion system

All animations are self-contained SVG (SMIL), so every part runs on one shared clock and they can't drift out of sync. They use a 64×64 viewBox like the mark.

| File | Use | Duration | Plays |
|---|---|---|---|
| `launch-build-in.svg` | App start, TV splash | 1.2 s | once, ends on the logo |
| `loading-light-leak.svg` | Loading / waiting (play stays whole) | 2.4 s | loops |
| `loading-light-leak-split.svg` | Alternative: play halves move with the doors | 2.4 s | loops |
| `playback-press-open.svg` | Starting playback (chosen) | 1.5 s | once, ends open |
| `playback-slide-apart.svg` | Alternative playback reveal: play fades, doors slide | 1.4 s | once, ends open |

Implementation notes:

- **Outline colour in animations.** In the animations the play symbol's outline is drawn in the background colour rather than cut out, because it moves independently of the doors. It reads the CSS variable `--mc-bg` (default `#121214`). When the SVG is inlined in a page, set `--mc-bg` to match the surface behind it; when used via `<img>`, the default applies.
- **Reveal content.** The playback animations contain a `<g id="reveal-content">` with a placeholder rectangle. Replace it with the poster (`<image … preserveAspectRatio="xMidYMid slice">`) or position the poster behind a transparent version.
- **Don't let motion delay the user.** If playback is ready before the reveal finishes, jump to the end state.
- **Light leak in the app.** The web app's loading mark (`LoadingMark` in `web/src/components/Brand.tsx`) starts the light at opacity 0 instead of the file's 0.25, so at rest the seam is dark like the logo's cut-out; at small sizes the dim seam made the doors read as one block and the play as an outlined icon.
- **Reduced motion.** SMIL ignores `prefers-reduced-motion`. In the app, check the setting and show the static mark (launch/loading) or cut straight to the poster (playback).
- **Native clients.** The motion is only translate, scale and opacity on four shapes, so it's straightforward to rebuild with Android animators, SwiftUI or Lottie. Timings and easing are in the SVG `keyTimes`/`keySplines`.

## Usage

- Minimum size: 16 px for the mark (use the favicon-16 version), 120 px wide for the lockup.
- Clear space: at least half the mark's width on all sides of the lockup.
- On light backgrounds, use the darker teal `#0F7A71` for the word "cabinet" (the light lockup already does).
- Don't recolour the cut-outs, add a frame, or put a line through the play symbol.
