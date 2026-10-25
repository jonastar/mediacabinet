import { useSyncExternalStore } from 'react'

function subscribe(onChange: () => void) {
  window.addEventListener('popstate', onChange)
  return () => window.removeEventListener('popstate', onChange)
}

export function usePath(): string {
  return useSyncExternalStore(subscribe, () => window.location.pathname)
}

export function navigate(to: string) {
  if (to === window.location.pathname) {
    return
  }
  window.history.pushState(null, '', to)
  window.dispatchEvent(new PopStateEvent('popstate'))
  window.scrollTo(0, 0)
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
