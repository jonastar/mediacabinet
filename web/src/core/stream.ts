import type Hls from 'hls.js'
import type { HlsConfig } from 'hls.js'
import type { components } from '../api/schema.d.ts'

type PlayPlan = components['schemas']['PlayPlan']
export type HlsClass = typeof Hls

/** Whether streams go through hls.js here, rather than the browser's own HLS. */
export const hasMediaSource = () => 'MediaSource' in window || 'ManagedMediaSource' in window

/** Plays, on engines where `play()` returns a promise and on older ones where it doesn't. */
export function startPlaying(el: HTMLVideoElement) {
  const playing: Promise<void> | undefined = el.play()
  if (playing) {
    playing.catch(() => undefined)
  }
}

export interface StreamOptions {
  /** Hands over hls.js, so the core doesn't depend on it. */
  loadHls: () => Promise<HlsClass>
  /** Called when the stream breaks: it couldn't be fetched, or couldn't be played. */
  onFatal: (message: string, cause: 'network' | 'decode') => void
  hlsConfig?: Partial<HlsConfig>
  /** The player, once there is one, for measuring. */
  onPlayer?: (player: Hls, hls: HlsClass) => void
  /** Where a plan's file is, for clients that are cross-site or send a token. */
  fileUrl?: (url: string) => string
  /** Where a plan's playlist is, likewise. */
  playlistUrl?: (url: string) => string
}

/**
 * Points the video at a plan's stream: the file itself, or HLS through hls.js.
 * hls.js is used wherever Media Source exists, even where the browser claims
 * HLS itself: Chrome's own HLS can't read these streams. Native HLS is the
 * fallback for iPhones without Media Source. Returns how to detach it.
 */
export function attachStream(el: HTMLVideoElement, plan: PlayPlan, options: StreamOptions) {
  const { loadHls, onFatal, hlsConfig, onPlayer, fileUrl, playlistUrl } = options
  let cancelled = false
  let destroy = () => {}
  // Direct play starts at the resume point; an HLS stream starts there already.
  const startAt = plan.mode === 'direct' ? plan.start_seconds : 0
  const begin = () => {
    if (startAt > 0) {
      el.currentTime = startAt
    }
    startPlaying(el)
  }

  const isHls = plan.url.indexOf('.m3u8') === plan.url.length - 5
  if (isHls && hasMediaSource()) {
    loadHls().then(
      (HlsPlayer) => {
        if (cancelled) {
          return
        }
        // An EVENT playlist grows as ffmpeg works; play it from the start.
        const player = new HlsPlayer({
          startPosition: 0,
          liveDurationInfinity: false,
          ...hlsConfig,
        })
        onPlayer?.(player, HlsPlayer)
        player.on(HlsPlayer.Events.MANIFEST_PARSED, begin)
        let recovered = false
        player.on(HlsPlayer.Events.ERROR, (_event, data) => {
          if (!data.fatal) {
            return
          }
          if (data.type === HlsPlayer.ErrorTypes.MEDIA_ERROR && !recovered) {
            recovered = true
            player.recoverMediaError()
            return
          }
          onFatal(
            data.details,
            data.type === HlsPlayer.ErrorTypes.NETWORK_ERROR ? 'network' : 'decode',
          )
        })
        player.loadSource(playlistUrl ? playlistUrl(plan.url) : plan.url)
        player.attachMedia(el)
        destroy = () => player.destroy()
      },
      () => {
        if (!cancelled) {
          onFatal('the streaming code could not be loaded', 'network')
        }
      },
    )
  } else {
    const url = isHls ? playlistUrl?.(plan.url) : fileUrl?.(plan.url)
    // Removes itself: older engines don't know addEventListener's `once`.
    const onMetadata = () => {
      el.removeEventListener('loadedmetadata', onMetadata)
      begin()
    }
    el.setAttribute('src', url ?? plan.url)
    el.addEventListener('loadedmetadata', onMetadata)
    destroy = () => el.removeEventListener('loadedmetadata', onMetadata)
  }
  return () => {
    cancelled = true
    destroy()
    el.removeAttribute('src')
    el.load()
  }
}
