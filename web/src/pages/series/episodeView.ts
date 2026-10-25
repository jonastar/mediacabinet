import type { Episode } from '../../api/client'
import {
  EMPTY_TONE,
  ago,
  airDate,
  episodeLabel,
  episodeTone,
  filesSummary,
  matchSummary,
  minutesLeft,
  progress,
} from '../../lib/format'
import type { Mode } from '../../lib/prefs'
import { episodeBadge, type Badge } from '../../lib/wording'

/** What a row or card shows for one episode, in one mode. */
export interface EpisodeView {
  key: string
  episode: Episode
  label: string
  meta: string
  tech: string
  badge: Badge
  tone: string
  /** Placeholder thumbnails are tinted when there's something to play. */
  hasArt: boolean
  progress: number
  isUpNext: boolean
}

export function episodeView(
  e: Episode,
  index: number,
  upNext: number | null | undefined,
  mode: Mode,
): EpisodeView {
  const isUpNext = index === upNext
  const watch = e.watch
  const played = watch?.played ?? false
  const left = watch && !played ? minutesLeft(e.runtime_seconds, watch.position_seconds) : null
  const meta = [
    airDate(e.airs_at),
    mode === 'watch' && played ? 'watched' : null,
    mode === 'watch' && left ? `${left} min left` : null,
  ]
    .filter(Boolean)
    .join(' · ')

  const status = e.statuses.find((s) => !s.stale)
  const statusText = status
    ? [
        status.source,
        [status.status, percent(status.detail)].filter(Boolean).join(' '),
        ago(status.updated_at),
      ].join(' · ')
    : null
  const tech =
    [filesSummary(e.files), matchSummary(e.files), statusText].filter(Boolean).join('  ·  ') ||
    'no file linked'

  const hasArt = e.availability === 'available'
  return {
    key: `${index}:${e.item_id}`,
    episode: e,
    label: episodeLabel(e.number),
    meta,
    tech,
    badge: episodeBadge(e, mode, isUpNext && mode === 'watch'),
    tone: hasArt ? episodeTone(e.item_id) : EMPTY_TONE,
    hasArt,
    progress: progress(e.runtime_seconds, watch?.position_seconds ?? 0, played),
    isUpNext,
  }
}

function percent(detail: unknown): string | null {
  const p = (detail as { progress?: number } | null)?.progress
  return typeof p === 'number' ? `${Math.round(p * 100)}%` : null
}
