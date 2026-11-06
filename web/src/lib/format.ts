import type { LinkedFile } from '../api/client'

export { episodeLabel } from '../core/episode'

const dateFormat = new Intl.DateTimeFormat(undefined, {
  day: 'numeric',
  month: 'short',
  year: 'numeric',
})
const dayTimeFormat = new Intl.DateTimeFormat(undefined, {
  weekday: 'short',
  day: 'numeric',
  month: 'short',
  hour: '2-digit',
  minute: '2-digit',
})
const dayFormat = new Intl.DateTimeFormat(undefined, {
  weekday: 'short',
  day: 'numeric',
  month: 'short',
})
const weekdayFormat = new Intl.DateTimeFormat(undefined, { weekday: 'short' })
const timeFormat = new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' })

const date = (seconds: number) => new Date(seconds * 1000)

/**
 * Past: `4 Apr 2019`. Future: `Fri 30 Oct, 17:30`, in the viewer's time zone, or
 * `Fri 30 Oct` when the provider only knows the date.
 */
export function airDate(
  airsAt: number | null | undefined,
  timeKnown = true,
  now = Date.now() / 1000,
): string {
  if (airsAt == null) {
    return 'No air date yet'
  }
  if (airsAt <= now) {
    return dateFormat.format(airDay(airsAt, timeKnown))
  }
  return timeKnown ? dayTimeFormat.format(date(airsAt)) : dayFormat.format(airDay(airsAt, false))
}

/**
 * Date-only air times are stored as the end of that day in UTC; step back into
 * the day itself, so time zones west of UTC don't show the next day.
 */
function airDay(seconds: number, timeKnown: boolean) {
  if (timeKnown) {
    return date(seconds)
  }
  const d = date(seconds - 12 * 3600)
  return new Date(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate())
}

export const airDayLabel = (seconds: number, timeKnown: boolean) =>
  timeKnown ? dayTimeFormat.format(date(seconds)) : dayFormat.format(airDay(seconds, false))
export const airShortDay = (seconds: number, timeKnown: boolean) =>
  dayFormat.format(airDay(seconds, timeKnown))
export const airWeekday = (seconds: number, timeKnown: boolean) =>
  weekdayFormat.format(airDay(seconds, timeKnown))

/** The pieces of a day tile: `Sun` and `1`, or `Today` for today. */
export function airDayTile(seconds: number, timeKnown: boolean, now = new Date()) {
  const d = airDay(seconds, timeKnown)
  const today = d.toDateString() === now.toDateString()
  return { label: today ? 'Today' : weekdayFormat.format(d), day: d.getDate(), today }
}

export const shortDay = (seconds: number) => dayFormat.format(date(seconds))
export const dayTimeLabel = (seconds: number) => dayTimeFormat.format(date(seconds))
export const weekday = (seconds: number) => weekdayFormat.format(date(seconds))
export const time = (seconds: number) => timeFormat.format(date(seconds))

export function minutesLeft(runtime: number | null | undefined, position: number): number | null {
  if (!runtime || position <= 0) {
    return null
  }
  return Math.max(1, Math.round((runtime - position) / 60))
}

export function progress(runtime: number | null | undefined, position: number, played: boolean) {
  if (played) {
    return 1
  }
  if (!runtime || position <= 0) {
    return 0
  }
  return Math.min(1, position / runtime)
}

export function bytes(n: number): string {
  if (n >= 1e9) {
    return `${(n / 1e9).toFixed(1)} GB`
  }
  if (n >= 1e6) {
    return `${Math.round(n / 1e6)} MB`
  }
  return `${Math.max(1, Math.round(n / 1e3))} KB`
}

export function resolution(height: number | null | undefined): string | null {
  if (!height) {
    return null
  }
  const steps = [2160, 1440, 1080, 720, 576, 480]
  const step = steps.find((s) => height >= s * 0.9)
  return `${step ?? height}p`
}

const CODECS: Record<string, string> = { hevc: 'HEVC', h264: 'H.264', av1: 'AV1', vp9: 'VP9' }
const HDR: Record<string, string> = { hdr10: 'HDR10', hlg: 'HLG', dolby_vision: 'Dolby Vision' }

/** `1080p HEVC · 1.2 GB`, or `2 versions · 2160p HDR10, 1080p`. */
export function filesSummary(files: LinkedFile[], now = Date.now() / 1000): string | null {
  const present = files.filter((f) => f.status === 'present')
  if (present.length > 1) {
    const versions = present.map((f) =>
      [resolution(f.height), f.hdr_format ? HDR[f.hdr_format] : null].filter(Boolean).join(' '),
    )
    return `${present.length} versions · ${versions.join(', ')}`
  }
  if (present.length === 1) {
    const f = present[0]
    const codec = f.video_codec ? (CODECS[f.video_codec] ?? f.video_codec.toUpperCase()) : null
    const hdr = f.hdr_format ? HDR[f.hdr_format] : null
    return [[resolution(f.height), hdr, codec].filter(Boolean).join(' '), bytes(f.size)]
      .filter(Boolean)
      .join(' · ')
  }
  const missing = files.find((f) => f.status === 'missing')
  if (missing) {
    const seen =
      now - missing.last_seen < 86400 ? time(missing.last_seen) : shortDay(missing.last_seen)
    return `${missing.root_path} · last seen ${seen}`
  }
  return null
}

/** `nfo 1.00 · pinned`; user links have no confidence to show. */
export function matchSummary(files: LinkedFile[]): string | null {
  const f = files.find((x) => x.status === 'present') ?? files[0]
  if (!f) {
    return null
  }
  const source = f.link_source === 'user' ? 'user' : `${f.link_source} ${f.confidence.toFixed(2)}`
  return f.pinned ? `${source} · pinned` : source
}

export function ago(seconds: number, now = Date.now() / 1000): string {
  const minutes = Math.round((now - seconds) / 60)
  if (minutes < 1) {
    return 'just now'
  }
  if (minutes < 60) {
    return `${minutes} min ago`
  }
  const hours = Math.round(minutes / 60)
  return hours < 24 ? `${hours} h ago` : `${Math.round(hours / 24)} d ago`
}

/** Placeholder artwork colours from the mockups, until real artwork arrives. */
const TONES = [
  '#3A1C20',
  '#1E3247',
  '#45391F',
  '#1E3F31',
  '#33284A',
  '#1B3C42',
  '#42263A',
  '#2B3324',
]
const EPISODE_TONES = ['#4A2329', '#3F2A22', '#2E2A44', '#23383A', '#3E2A1E', '#2C3346']

function hash(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) {
    h = (h * 31 + s.charCodeAt(i)) | 0
  }
  return Math.abs(h)
}

export const seriesTone = (title: string) => TONES[hash(title) % TONES.length]
export const episodeTone = (itemId: number) => EPISODE_TONES[itemId % EPISODE_TONES.length]
export const EMPTY_TONE = '#1A1D24'

/** A length for display, e.g. `1 h 42 min`. Null when it isn't known. */
export function runtime(seconds: number | null | undefined) {
  if (!seconds) {
    return null
  }
  const m = Math.round(seconds / 60)
  if (m === 0) {
    return 'Under 1 min'
  }
  return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`
}
