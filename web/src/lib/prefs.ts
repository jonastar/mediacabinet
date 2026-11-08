import { useCallback, useSyncExternalStore } from 'react'
import { useSession } from './session'

/**
 * Small per-browser preferences, in localStorage; reads fall back to the
 * default when storage is unavailable.
 */
const listeners = new Set<() => void>()

function read(key: string): string | null {
  try {
    return window.localStorage.getItem(key)
  } catch {
    return null
  }
}

function write(key: string, value: string) {
  try {
    window.localStorage.setItem(key, value)
  } catch {
    // Private mode or blocked storage: the choice lasts for this page only.
  }
  memory.set(key, value)
  listeners.forEach((l) => l())
}

const memory = new Map<string, string>()

function subscribe(onChange: () => void) {
  listeners.add(onChange)
  return () => listeners.delete(onChange)
}

function usePref<T extends string>(key: string, options: readonly T[], fallback: T) {
  const value = useSyncExternalStore(subscribe, () => memory.get(key) ?? read(key))
  const current = options.includes(value as T) ? (value as T) : fallback
  const set = useCallback((next: T) => write(key, next), [key])
  return [current, set] as const
}

export type Mode = 'watch' | 'manage'
export type Layout = 'list' | 'grid'

/**
 * Watch is for viewing; Manage shows system states and technical detail.
 * Admins switch it with the Edit mode switch on media pages; it's remembered
 * and applies everywhere media is shown, and only admins can manage.
 */
export function useMode() {
  const [mode, setMode] = usePref<Mode>('mc.mode', ['watch', 'manage'], 'watch')
  const session = useSession()
  const admin = session.state === 'signedIn' && session.user.is_admin
  return [admin ? mode : 'watch', setMode] as const
}

export const useEpisodeLayout = () => usePref<Layout>('mc.episodeLayout', ['list', 'grid'], 'list')
