import type { Episode } from '../api/client'
import type { Mode } from './prefs'
import { shortDay } from './format'

/** Badge tones map to the state colours in index.css. */
export type Tone = 'ready' | 'watched' | 'next' | 'available' | 'missing' | 'upcoming' | 'neutral'

export interface Badge {
  label: string
  tone: Tone
}

/** An external tool's live status, e.g. "Downloading 40%". Stale statuses are hidden. */
export function liveStatus(e: Episode): string | null {
  const s = e.statuses.find((x) => !x.stale)
  if (!s) {
    return null
  }
  const name = s.status.charAt(0).toUpperCase() + s.status.slice(1)
  const p = (s.detail as { progress?: number } | null)?.progress
  return typeof p === 'number' ? `${name} ${Math.round(p * 100)}%` : name
}

/** The design's wording per state: friendly in Watch mode, system states in Manage. */
export function episodeBadge(e: Episode, mode: Mode, isUpNext: boolean): Badge {
  if (mode === 'manage') {
    switch (e.availability) {
      case 'available':
        return { label: 'available', tone: 'available' }
      case 'missing':
        return { label: 'missing', tone: 'missing' }
      case 'not_acquired':
        return { label: 'not acquired', tone: 'neutral' }
      case 'upcoming':
        return { label: 'upcoming', tone: 'upcoming' }
      case 'unknown':
        return { label: 'unknown', tone: 'neutral' }
    }
  }
  if (isUpNext) {
    return { label: 'Up next', tone: 'next' }
  }
  switch (e.availability) {
    case 'available':
      return e.watch?.played
        ? { label: 'Watched', tone: 'watched' }
        : { label: 'Ready', tone: 'ready' }
    case 'missing':
      return { label: 'File offline', tone: 'missing' }
    case 'not_acquired': {
      const status = liveStatus(e)
      return status
        ? { label: status, tone: 'upcoming' }
        : { label: 'Not in library', tone: 'neutral' }
    }
    case 'upcoming':
      return { label: `Airs ${shortDay(e.airs_at ?? 0)}`, tone: 'upcoming' }
    case 'unknown':
      return { label: 'Date TBA', tone: 'neutral' }
  }
}
