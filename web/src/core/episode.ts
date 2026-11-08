import type { components } from '../api/schema.d.ts'

type EpisodeNumber = components['schemas']['EpisodeNumber']

const pad = (n: number) => String(n).padStart(2, '0')

/** `S01E03`, `03` or `E03`, following the ordering the number comes from. */
export function episodeLabel(n: EpisodeNumber | null | undefined): string {
  if (!n || n.number == null) {
    return '—'
  }
  if (n.season != null && n.style !== 'absolute') {
    return `S${pad(n.season)}E${pad(n.number)}`
  }
  return n.style === 'playlist' ? `E${pad(n.number)}` : pad(n.number)
}

/** `S01E03 · Rabbot`, or just the title when there's no number to show. */
export function episodeLine(episode: { number?: EpisodeNumber | null; title: string }): string {
  if (episode.number?.number == null) {
    return episode.title
  }
  return `${episodeLabel(episode.number)} · ${episode.title}`
}

/** The season an episode is in, or null when its ordering has no seasons. */
export const seasonOf = (n: EpisodeNumber | null | undefined): number | null =>
  n?.style === 'season' ? (n.season ?? null) : null

export const seasonName = (season: number) => (season === 0 ? 'Specials' : `Season ${season}`)
