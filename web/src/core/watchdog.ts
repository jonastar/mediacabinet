/** How long video may have data to play and still show nothing. */
export const NO_PICTURE_SECONDS = 8
/** Enough buffered that not playing can't be the network's fault. */
const BUFFERED_ENOUGH_SECONDS = 2
/** Played this much, so it's really playing, and not just a first frame. */
const PLAYED_SECONDS = 0.2

function bufferedAhead(el: HTMLVideoElement) {
  for (let i = 0; i < el.buffered.length; i++) {
    if (el.buffered.start(i) <= el.currentTime && el.currentTime <= el.buffered.end(i)) {
      return el.buffered.end(i) - el.currentTime
    }
  }
  return 0
}

/**
 * Tells whether a stream starts playing: `onStarted` once it does, or `onStuck`
 * if it has data and shows nothing. A slow network or a browser blocking
 * autoplay isn't stuck. Returns how to stop watching.
 */
export function watchForPicture(
  el: HTMLVideoElement,
  onStarted: () => void,
  onStuck: () => void,
): () => void {
  let stuck = 0
  const timer = window.setInterval(() => {
    if (el.played.length > 0 && el.played.end(0) - el.played.start(0) > PLAYED_SECONDS) {
      window.clearInterval(timer)
      onStarted()
      return
    }
    const waiting =
      !el.paused && !el.seeking && !document.hidden && bufferedAhead(el) >= BUFFERED_ENOUGH_SECONDS
    stuck = waiting ? stuck + 1 : 0
    if (stuck >= NO_PICTURE_SECONDS) {
      window.clearInterval(timer)
      onStuck()
    }
  }, 1000)
  return () => window.clearInterval(timer)
}
