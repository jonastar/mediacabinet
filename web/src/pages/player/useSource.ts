import { useEffect, type RefObject } from 'react'
import type { PlayPlan } from '../../api/client'
import { attachStream, hasMediaSource } from '../../core/stream'
import type { Meter } from './meter'

const loadHls = () => import('hls.js').then(({ default: Hls }) => Hls)

/**
 * Starts loading hls.js before it's known to be needed, so a stream doesn't
 * wait for it after the server answers. Direct play just leaves it cached.
 */
export function preloadHls() {
  if (hasMediaSource()) {
    loadHls().catch(() => undefined)
  }
}

/**
 * Points the video at a plan's stream (see `attachStream`). hls.js reports its
 * segment downloads to `meter`.
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
    return attachStream(el, plan, {
      loadHls,
      onFatal,
      onPlayer: (player, Hls) => meter.attach(player, Hls.Events),
    })
  }, [video, plan, onFatal, meter])
}
