import { useEffect } from 'react'
import { api } from '../../api/client'
import { useApi } from '../../api/useApi'
import { useLiveEvents } from '../../lib/live'

/** All libraries, kept current: live events, and every second while anything scans. */
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
  const scanning = libraries.data?.some((l) => l.roots.some((r) => r.scanning)) ?? false
  useEffect(() => {
    if (!scanning) {
      return
    }
    const timer = window.setInterval(reload, 1000)
    return () => window.clearInterval(timer)
  }, [scanning, reload])
  return libraries
}
