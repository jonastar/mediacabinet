import type { ReactNode } from 'react'

export interface Fact {
  label: string
  value: ReactNode
}

/** Only facts that have a value. */
export function facts(...list: Fact[]): Fact[] {
  return list.filter((f) => f.value != null && f.value !== '' && f.value !== false)
}

const PROVIDERS: Record<string, string> = {
  tmdb: 'TMDB',
  tvdb: 'TVDB',
  anidb: 'AniDB',
  imdb: 'IMDb',
  trakt: 'Trakt',
  computed: 'derived',
}
export const provider = (p: string) => PROVIDERS[p] ?? p

/** The fields "Lock fields" keeps through refreshes. */
export const LOCKABLE = ['title', 'year', 'overview']

/** `TMDB · TVDB`, primary first. */
export function matchedTo(ids: { provider: string; is_primary: boolean }[]): string {
  const names = [...ids]
    .sort((a, b) => Number(b.is_primary) - Number(a.is_primary))
    .map((x) => provider(x.provider))
  return [...new Set(names)].join(' · ') || 'Not matched'
}

export const lockedLabel = (fields: string[]) => fields.join(', ') || 'None'

/** `Returning series`, `Ended`. */
export function seriesStatus(status: string | null | undefined): string | null {
  if (!status) {
    return null
  }
  if (status.toLowerCase().startsWith('returning')) {
    return 'Returning series'
  }
  return status.charAt(0).toUpperCase() + status.slice(1)
}
