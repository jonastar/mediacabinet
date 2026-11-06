import { useCallback, useEffect, useRef, type RefObject } from 'react'
import type { ClientCaps, PlayPlan } from '../../api/client'
import { nextFailure, without, type Failure } from '../../core/fallback'
import { rememberFailures } from '../../core/caps'
import { NO_PICTURE_SECONDS, watchForPicture } from '../../core/watchdog'

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
    const el = video.current
    if (!el) {
      return
    }
    return watchForPicture(
      el,
      () => {
        started.current = true
        rememberFailures(unproven.current)
        unproven.current = []
      },
      () => decodeFailed(`no picture after ${NO_PICTURE_SECONDS} seconds`),
    )
  }, [plan, video, decodeFailed])

  const reset = useCallback(() => {
    unproven.current = []
  }, [])

  return { decodeFailed, reset }
}
