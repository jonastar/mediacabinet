import { useEffect, useSyncExternalStore } from 'react'

function subscribe(onChange: () => void) {
  window.addEventListener('popstate', onChange)
  return () => window.removeEventListener('popstate', onChange)
}

export function usePath(): string {
  return useSyncExternalStore(subscribe, () => window.location.pathname)
}

/** A query string parameter of the current URL. */
export function useQueryParam(name: string): string | null {
  const search = useSyncExternalStore(subscribe, () => window.location.search)
  return new URLSearchParams(search).get(name)
}

/** Goes to `to`; `replace` swaps the current history entry instead of adding one. */
export function navigate(to: string, { replace = false }: { replace?: boolean } = {}) {
  if (to === window.location.pathname + window.location.search) {
    return
  }
  if (replace) {
    window.history.replaceState(null, '', to)
  } else {
    window.history.pushState(null, '', to)
  }
  window.dispatchEvent(new PopStateEvent('popstate'))
  window.scrollTo(0, 0)
}

/** Replaces the URL with `path` (keeping the query) once it's known, without a history entry. */
export function useCanonicalPath(path: string | null) {
  const current = usePath()
  useEffect(() => {
    if (path && path !== current) {
      navigate(path + window.location.search, { replace: true })
    }
  }, [path, current])
}

/** Where a series or movie's page is: under its library when it has one. */
export function itemPath(kind: 'series' | 'movie', id: number, libraryId?: number | null) {
  if (libraryId) {
    return `/library/${libraryId}/${id}`
  }
  return kind === 'series' ? `/tv/${id}` : `/movies/${id}`
}

/** A folder of a folder library; an empty `path` is the library's top. */
export function folderPath(libraryId: number, path: string) {
  return path ? `/library/${libraryId}?path=${encodeURIComponent(path)}` : `/library/${libraryId}`
}

/** Where an episode's page is: under its series' page. */
export function episodePath(seriesId: number, episodeId: number, libraryId?: number | null) {
  return `${itemPath('series', seriesId, libraryId)}/${episodeId}`
}

/** A series' page with one of its seasons showing. */
export function seasonPath(seriesPath: string, season: number) {
  return `${seriesPath}?season=${season}`
}

/** Matches `/tv/:id`-style patterns. Returns the params, or null. */
export function match(pattern: string, path: string): Record<string, string> | null {
  const want = pattern.split('/')
  const have = path.replace(/\/+$/, '').split('/')
  if (want.length !== have.length) {
    return null
  }
  const params: Record<string, string> = {}
  for (let i = 0; i < want.length; i++) {
    if (want[i].startsWith(':')) {
      params[want[i].slice(1)] = decodeURIComponent(have[i])
    } else if (want[i] !== have[i]) {
      return null
    }
  }
  return params
}
