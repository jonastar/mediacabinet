import type { LibraryKind } from '../../api/client'

/** The item folder on its own, at the top of the root: the usual layout. */
export const DEFAULT_ABOVE: Record<LibraryKind, string> = {
  tv: '{series}',
  movies: '{movie} ({year})',
}

/**
 * A template split after its item folder: the levels you choose (grouping and
 * the item folder), and the ones below it, which the matcher reads on its own.
 */
export function splitTemplate(template: string, kind: LibraryKind) {
  const levels = template.replace(/^\/+|\/+$/g, '').split('/')
  const token = kind === 'movies' ? '{movie' : '{series'
  const item = levels.findIndex((l) => l.includes(token))
  if (item < 0) {
    return { above: template, below: '' }
  }
  return { above: levels.slice(0, item + 1).join('/'), below: levels.slice(item + 1).join('/') }
}

export const joinTemplate = (above: string, below: string) => (below ? `${above}/${below}` : above)
