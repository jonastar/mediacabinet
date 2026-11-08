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
  runtime,
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
    airDate(e.airs_at, e.airs_time_known),
    runtime(e.runtime_seconds),
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
    [
      filesSummary(e.files),
      matchSummary(e.files),
      e.wanted_reason === 'upgrade' ? 'upgrade wanted: below the quality cutoff' : null,
      statusText,
    ]
      .filter(Boolean)
      .join('  ·  ') || 'no file linked'

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

/** One season's episodes; `season` is null when the ordering has no seasons. */
export interface SeasonGroup {
  key: string
  season: number | null
  name: string
  views: EpisodeView[]
}

export const seasonName = (season: number) => (season === 0 ? 'Specials' : `Season ${season}`)

/** Episodes by season in the ordering's order, with specials last. */
export function groupBySeason(views: EpisodeView[]): SeasonGroup[] {
  const groups = new Map<number | null, SeasonGroup>()
  for (const v of views) {
    const number = v.episode.number
    const season = number?.style === 'season' ? (number.season ?? null) : null
    const group = groups.get(season)
    if (group) {
      group.views.push(v)
    } else {
      groups.set(season, {
        key: String(season),
        season,
        name: season === null ? 'Episodes' : seasonName(season),
        views: [v],
      })
    }
  }
  const all = [...groups.values()]
  return [...all.filter((g) => g.season !== 0), ...all.filter((g) => g.season === 0)]
}
