const MINUTE = 60
const HOUR = 3600
const DAY = 86400

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`

export interface Countdown {
  /** `in 1 day and 2 hours`, `in 3 hours and 12 minutes`, `in 12 minutes`, `Airing now`. */
  long: string
  /** `in 1d 2h`, `in 3h 12m`, `in 12m`, `Now`. */
  short: string
}

/**
 * Time until an episode airs. A date with no known time is stored as the end of
 * that day in UTC, so it only counts whole days, by the calendar of that date.
 */
export function countdown(airsAt: number, timeKnown: boolean, now: number): Countdown {
  if (!timeKnown) {
    return wholeDays(airsAt, now)
  }
  const left = Math.max(0, airsAt - now)
  if (left === 0) {
    return { long: 'Airing now', short: 'Now' }
  }
  const days = Math.floor(left / DAY)
  const hours = Math.floor(left / HOUR) % 24
  const minutes = Math.floor(left / MINUTE) % 60
  if (days > 0) {
    return {
      long: `in ${plural(days, 'day')} and ${plural(hours, 'hour')}`,
      short: `in ${days}d ${hours}h`,
    }
  }
  if (hours > 0) {
    return {
      long: `in ${plural(hours, 'hour')} and ${plural(minutes, 'minute')}`,
      short: `in ${hours}h ${minutes}m`,
    }
  }
  return { long: `in ${plural(minutes, 'minute')}`, short: `in ${minutes}m` }
}

function wholeDays(airsAt: number, now: number): Countdown {
  const air = new Date((airsAt - 12 * HOUR) * 1000)
  const today = new Date(now * 1000)
  const days = Math.round(
    (Date.UTC(air.getUTCFullYear(), air.getUTCMonth(), air.getUTCDate()) -
      Date.UTC(today.getFullYear(), today.getMonth(), today.getDate())) /
      (DAY * 1000),
  )
  if (days <= 0) {
    return { long: 'Airing today', short: 'Today' }
  }
  return { long: `in ${plural(days, 'day')}`, short: `in ${days}d` }
}
