import { useState } from 'react'
import { api, type CalendarEntry } from '../api/client'
import { useApi } from '../api/useApi'
import { Badge } from '../components/Badge'
import { Link } from '../components/Link'
import { Segmented } from '../components/Segmented'
import { Failed, Loading } from '../components/Status'
import { episodeLabel, seriesTone, time } from '../lib/format'
import { useMode } from '../lib/prefs'
import type { Badge as BadgeValue } from '../lib/wording'
import styles from './Agenda.module.css'
import { LOCALE } from '../core/locale'

const DAY = 86400
type Range = 'upcoming' | 'past'

const dayFormat = new Intl.DateTimeFormat(LOCALE, {
  weekday: 'long',
  day: 'numeric',
  month: 'long',
})

/** The local calendar day an entry falls on; date-only entries use their UTC date. */
function localDay(e: CalendarEntry): Date {
  const d = new Date(e.airs_time_known ? e.airs_at * 1000 : (e.airs_at - DAY / 2) * 1000)
  return e.airs_time_known
    ? new Date(d.getFullYear(), d.getMonth(), d.getDate())
    : new Date(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate())
}

function dayTitle(day: Date): { text: string; today: boolean } {
  const now = new Date()
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  const diff = Math.round((day.getTime() - today.getTime()) / (DAY * 1000))
  if (diff === 0) {
    return { text: `Today · ${dayFormat.format(day)}`, today: true }
  }
  if (diff === 1) {
    return { text: `Tomorrow · ${dayFormat.format(day)}`, today: false }
  }
  if (diff === -1) {
    return { text: `Yesterday · ${dayFormat.format(day)}`, today: false }
  }
  return { text: dayFormat.format(day), today: false }
}

function badge(e: CalendarEntry, manage: boolean): BadgeValue {
  if (manage) {
    if (e.wanted) {
      return { label: 'wanted', tone: 'missing' }
    }
    return e.monitored
      ? { label: 'monitored', tone: 'available' }
      : { label: 'unmonitored', tone: 'neutral' }
  }
  switch (e.availability) {
    case 'available':
      return { label: 'Ready', tone: 'ready' }
    case 'missing':
      return { label: 'File offline', tone: 'missing' }
    case 'not_acquired':
      return { label: 'Not in library', tone: 'neutral' }
    default:
      return { label: 'Upcoming', tone: 'upcoming' }
  }
}

export function CalendarPage() {
  const [mode] = useMode()
  const [range, setRange] = useState<Range>('upcoming')
  // Taken once, rounded to the hour, so the request key is stable across renders.
  const [now] = useState(() => Math.floor(Date.now() / 1000 / 3600) * 3600)
  const [from, to] = range === 'upcoming' ? [now - DAY, now + 28 * DAY] : [now - 14 * DAY, now]
  const entries = useApi(`calendar:${from}:${to}`, () => api.calendar(from, to))

  if (entries.error) {
    return <Failed error={entries.error} onRetry={entries.reload} />
  }
  const days = new Map<number, CalendarEntry[]>()
  for (const e of entries.data ?? []) {
    const key = localDay(e).getTime()
    days.set(key, [...(days.get(key) ?? []), e])
  }
  const ordered = [...days.entries()].sort(([a], [b]) => (range === 'upcoming' ? a - b : b - a))

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <div className={styles.header}>
          <div className={styles.heading}>
            <h1>Calendar</h1>
            <p className={styles.lead}>
              {range === 'upcoming'
                ? 'Airing in the next four weeks.'
                : 'Aired in the last two weeks.'}{' '}
              Times are yours; dates without a time come from the provider.
            </p>
          </div>
          <Segmented
            label="Range"
            variant="pills"
            value={range}
            onChange={setRange}
            options={[
              { value: 'upcoming', label: 'Upcoming' },
              { value: 'past', label: 'Recently aired' },
            ]}
          />
        </div>
        {!entries.data && <Loading />}
        {entries.data && entries.data.length === 0 && (
          <div className={styles.empty}>Nothing airs in this range.</div>
        )}
        {ordered.map(([key, list]) => {
          const title = dayTitle(new Date(key))
          return (
            <section key={key} className={styles.group} aria-label={title.text}>
              <h2 className={`${styles.groupTitle} ${title.today ? styles.today : ''}`}>
                {title.text}
              </h2>
              <ul className={styles.list}>
                {list.map((e) => {
                  const name = e.series_title ?? e.title
                  const link = e.series_id
                    ? `/tv/${e.series_id}/${e.item_id}`
                    : `/movies/${e.item_id}`
                  return (
                    <li key={e.item_id}>
                      <Link to={link} className={styles.row}>
                        <span className={styles.tile} style={{ background: seriesTone(name) }}>
                          {e.airs_time_known ? time(e.airs_at) : <small>All day</small>}
                        </span>
                        <span className={styles.text}>
                          <span className={styles.title}>{name}</span>
                          <span className={styles.meta}>
                            {e.kind === 'movie'
                              ? 'Release'
                              : `${episodeLabel(e.number)} · ${e.title}`}
                          </span>
                        </span>
                        <span className={styles.badges}>
                          <Badge badge={badge(e, mode === 'manage')} small />
                        </span>
                      </Link>
                    </li>
                  )
                })}
              </ul>
            </section>
          )
        })}
      </div>
    </div>
  )
}
