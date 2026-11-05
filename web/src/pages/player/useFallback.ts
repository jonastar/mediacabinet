import { useCallback, useEffect, useRef, type RefObject } from 'react'
import type { ClientCaps, PlayPlan } from '../../api/client'
import { nextFailure, without, type Failure } from '../../core/fallback'
import { rememberFailures } from '../../lib/caps'

/** How long video may have data to play and still show nothing. */
const NO_PICTURE_SECONDS = 8
/** Enough buffered that not playing can't be the network's fault. */
const BUFFERED_ENOUGH_SECONDS = 2

function bufferedAhead(el: HTMLVideoElement) {
  for (let i = 0; i < el.buffered.length; i++) {
    if (el.buffered.start(i) <= el.currentTime && el.currentTime <= el.buffered.end(i)) {
      return el.buffered.end(i) - el.currentTime
    }
  }
  return 0
}

/**
 * Recovers when a device can't play what it said it could: asks again without
 * one thing at a time (see `core/fallback.ts`). A step is remembered for this
 * device only once playing actually works, so a broken file teaches nothing.
 * `restart` asks again with the capabilities it's given. Returns what to call
 * when the video can't be decoded, and how to drop steps not yet proven.
 */
export function useFallback(
  video: RefObject<HTMLVideoElement | null>,
  plan: PlayPlan | null,
  currentCaps: () => ClientCaps,
  restart: (caps: ClientCaps) => void,
  giveUp: (message: string) => void,
) {
  const unproven = useRef<Failure[]>([])
  const started = useRef(false)

  const decodeFailed = useCallback(
    (message: string) => {
      const caps = currentCaps()
      const failure = plan && nextFailure(plan, caps)
      if (!failure) {
        giveUp(message)
        return
      }
      // Something that broke partway through a file says little about the device.
      if (!started.current) {
        unproven.current.push(failure)
      }
      restart(without(caps, failure))
    },
    [plan, currentCaps, restart, giveUp],
  )

  useEffect(() => {
    if (!plan) {
      return
    }
    started.current = false
    let stuck = 0
    const timer = window.setInterval(() => {
      const el = video.current
      if (!el) {
        return
      }
      const played = el.played.length > 0 && el.played.end(0) - el.played.start(0) > 0.2
      if (played) {
        started.current = true
        rememberFailures(unproven.current)
        unproven.current = []
        window.clearInterval(timer)
        return
      }
      // Paused covers browsers that block playing until someone clicks.
      const waitingForPicture =
        !el.paused &&
        !el.seeking &&
        !document.hidden &&
        bufferedAhead(el) >= BUFFERED_ENOUGH_SECONDS
      stuck = waitingForPicture ? stuck + 1 : 0
      if (stuck >= NO_PICTURE_SECONDS) {
        window.clearInterval(timer)
        decodeFailed(`no picture after ${NO_PICTURE_SECONDS} seconds`)
      }
    }, 1000)
    return () => window.clearInterval(timer)
  }, [plan, video, decodeFailed])

  const reset = useCallback(() => {
    unproven.current = []
  }, [])

  return { decodeFailed, reset }
}
