import { useEffect } from 'react'
import { api } from '../../api/client'
import { useApi } from '../../api/useApi'
import { useLiveEvents } from '../../lib/live'

/** All libraries, kept current: live events, and every second while anything scans or refreshes. */
export function useLibraries() {
  const libraries = useApi('libraries', api.libraries)
  const reload = libraries.reload
  // Scans started elsewhere (schedule, watcher, *arr webhooks) show up as they finish.
  useLiveEvents(
    [
      'scan.finished',
      'root.added',
      'root.changed',
      'root.removed',
      'library.added',
      'library.removed',
    ],
    reload,
  )
  const busy = libraries.data?.some((l) => l.refreshing || l.roots.some((r) => r.scanning)) ?? false
  useEffect(() => {
    if (!busy) {
      return
    }
    const timer = window.setInterval(reload, 1000)
    return () => window.clearInterval(timer)
  }, [busy, reload])
  return libraries
}
