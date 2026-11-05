import { useEffect, type RefObject } from 'react'
import type { PlayPlan } from '../../api/client'
import type { Meter } from './meter'

/** Whether streams go through hls.js here, rather than the browser's own HLS. */
const mediaSource = () => 'MediaSource' in window || 'ManagedMediaSource' in window

/**
 * Starts loading hls.js before it's known to be needed, so a stream doesn't
 * wait for it after the server answers. Direct play just leaves it cached.
 */
export function preloadHls() {
  if (mediaSource()) {
    import('hls.js').catch(() => undefined)
  }
}

/**
 * Points the video at a plan's stream: the file itself, or HLS through hls.js
 * (loaded only when needed). hls.js is used wherever Media Source exists, even
 * where the browser claims HLS itself: Chrome's own HLS can't read these
 * streams. Native HLS is the fallback for iPhones without Media Source.
 * `onFatal` is called when the stream breaks, saying whether it couldn't be
 * fetched (`network`) or couldn't be played (`decode`), for the player to recover.
 * hls.js reports its segment downloads to `meter`.
 */
export function useSource(
  video: RefObject<HTMLVideoElement | null>,
  plan: PlayPlan | null,
  onFatal: (message: string, cause: 'network' | 'decode') => void,
  meter: Meter,
) {
  useEffect(() => {
    const el = video.current
    if (!el || !plan) {
      return
    }
    let cancelled = false
    let destroy = () => {}
    // Direct play starts at the resume point; an HLS stream starts there already.
    const startAt = plan.mode === 'direct' ? plan.start_seconds : 0
    const begin = () => {
      if (startAt > 0) {
        el.currentTime = startAt
      }
      el.play().catch(() => undefined)
    }

    const hls = plan.url.endsWith('.m3u8')
    if (hls && mediaSource()) {
      import('hls.js').then(({ default: Hls }) => {
        if (cancelled) {
          return
        }
        // An EVENT playlist grows as ffmpeg works; play it from the start.
        const player = new Hls({ startPosition: 0, liveDurationInfinity: false })
        meter.attach(player, Hls.Events)
        player.on(Hls.Events.MANIFEST_PARSED, begin)
        let recovered = false
        player.on(Hls.Events.ERROR, (_event, data) => {
          if (!data.fatal) {
            return
          }
          if (data.type === Hls.ErrorTypes.MEDIA_ERROR && !recovered) {
            recovered = true
            player.recoverMediaError()
            return
          }
          onFatal(data.details, data.type === Hls.ErrorTypes.NETWORK_ERROR ? 'network' : 'decode')
        })
        player.loadSource(plan.url)
        player.attachMedia(el)
        destroy = () => player.destroy()
      })
    } else {
      el.setAttribute('src', plan.url)
      el.addEventListener('loadedmetadata', begin, { once: true })
      destroy = () => el.removeEventListener('loadedmetadata', begin)
    }
    return () => {
      cancelled = true
      destroy()
      el.removeAttribute('src')
      el.load()
    }
  }, [video, plan, onFatal, meter])
}
