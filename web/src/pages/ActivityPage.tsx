import { useState } from 'react'
import { api, type ActivityEntry } from '../api/client'
import { useApi } from '../api/useApi'
import { ActivityList } from '../components/ActivityFeed'
import { Segmented } from '../components/Segmented'
import { Failed, Loading } from '../components/Status'
import { useLiveEvents } from '../lib/live'
import { useUser } from '../lib/session'
import styles from './Page.module.css'

type Filter = 'all' | 'added' | 'watching' | 'admin'

const KINDS: Record<Filter, string | undefined> = {
  all: undefined,
  added: 'items_added',
  watching: 'watched,stopped_watching',
  admin: [
    'user_created',
    'user_changed',
    'user_removed',
    'user_status',
    'login',
    'login_failed',
    'login_linked',
    'login_unlinked',
    'config_changed',
    'task_finished',
    'task_failed',
  ].join(','),
}

/** What happened: additions, what people watched, and for admins, admin entries. */
export function ActivityPage() {
  const user = useUser()
  const [filter, setFilter] = useState<Filter>('all')
  const [older, setOlder] = useState<{ entries: ActivityEntry[]; next: string | null } | null>(null)
  const [error, setError] = useState<string | null>(null)
  // All is about the library and watching, even for admins; they have Admin for the rest.
  const watchOnly = filter === 'all' || (filter === 'admin' && !user.is_admin)
  const kinds = watchOnly ? `${KINDS.added},${KINDS.watching}` : KINDS[filter]
  const page = useApi(`activity:${kinds ?? 'all'}`, () => api.activity({ kinds }))
  useLiveEvents(['activity.added'], page.reload)

  const choose = (f: Filter) => {
    setFilter(f)
    setOlder(null)
  }
  const next = older ? older.next : (page.data?.next ?? null)
  const loadOlder = async () => {
    setError(null)
    try {
      const more = await api.activity({ kinds, before: next ?? undefined })
      setOlder((o) => ({
        entries: [...(o?.entries ?? []), ...more.entries],
        next: more.next ?? null,
      }))
    } catch (e) {
      setError((e as Error).message)
    }
  }
  const options: { value: Filter; label: string }[] = [
    { value: 'all', label: 'All' },
    { value: 'added', label: 'Added' },
    { value: 'watching', label: 'Watching' },
  ]
  if (user.is_admin) {
    options.push({ value: 'admin', label: 'Admin' })
  }
  const seen = new Set(page.data?.entries.map((e) => e.id))
  const entries = [
    ...(page.data?.entries ?? []),
    ...(older?.entries.filter((e) => !seen.has(e.id)) ?? []),
  ]

  return (
    <div className={styles.page}>
      <div className={styles.section}>
        <h1 className={styles.title}>Activity</h1>
        <p className={styles.muted}>
          {filter === 'admin'
            ? 'Sign-ins, settings and tasks.'
            : 'New in the library, and what people are watching.'}
        </p>
      </div>
      <Segmented label="Show" variant="pills" value={filter} onChange={choose} options={options} />
      {page.error && <Failed error={page.error} onRetry={page.reload} />}
      {!page.data && !page.error && <Loading />}
      {page.data && (
        <section aria-label="Entries" className={styles.section}>
          <ActivityList entries={entries} empty="Nothing here yet." />
          {error && (
            <p className={styles.muted} role="alert">
              {error}
            </p>
          )}
          {next && (
            <button type="button" className={styles.more} onClick={loadOlder}>
              Show older
            </button>
          )}
        </section>
      )}
    </div>
  )
}
