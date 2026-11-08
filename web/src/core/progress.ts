/** Where someone is in an episode or movie, as the server reports it. */
interface Watch {
  played: boolean
  position_seconds: number
}

/** How far through it someone is, from 0 to 1: 1 once it's played, 0 until it's started. */
export function progress(runtime: number | null | undefined, position: number, played: boolean) {
  if (played) {
    return 1
  }
  if (!runtime || position <= 0) {
    return 0
  }
  return Math.min(1, position / runtime)
}

/** Whole minutes still to watch, at least 1; null when it isn't started or the length is unknown. */
export function minutesLeft(runtime: number | null | undefined, position: number): number | null {
  if (!runtime || position <= 0) {
    return null
  }
  return Math.max(1, Math.round((runtime - position) / 60))
}

/** Started and not finished, so it resumes rather than plays from the start. */
export const isStarted = (watch: Watch | null | undefined) =>
  !!watch && !watch.played && watch.position_seconds > 0
