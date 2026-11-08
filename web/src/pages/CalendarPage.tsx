import { useState, type CSSProperties } from 'react'
import { api, type CalendarEntry } from '../api/client'
import { useApi } from '../api/useApi'
import { Badge } from '../components/Badge'
import {
  ArrowLeftIcon,
  ArrowRightIcon,
  ChevronDownIcon,
  GridIcon,
  ListIcon,
} from '../components/Icons'
import { Link } from '../components/Link'
import { Segmented } from '../components/Segmented'
import { Failed, Loading } from '../components/Status'
import { LOCALE } from '../core/locale'
import {
  addMonths,
  airDay,
  firstWeekday,
  monthGrid,
  monthParam,
  parseMonth,
  sameMonth,
  startOfDay,
  type Month,
} from '../lib/calendar'
import { episodeLabel, seriesTone, time } from '../lib/format'
import { useMode } from '../lib/prefs'
import { navigate, useQueryParam } from '../lib/router'
import type { Badge as BadgeValue } from '../lib/wording'
import agenda from './Agenda.module.css'
import styles from './CalendarPage.module.css'

const DAY = 86400
const FIRST_DAY = firstWeekday(LOCALE)
/** Chips a day cell shows before it folds the rest into "+N more". */
const CHIPS = 3

type View = 'month' | 'list'

const monthName = new Intl.DateTimeFormat(LOCALE, { month: 'long' })
const MONTHS = Array.from({ length: 12 }, (_, i) => monthName.format(new Date(2026, i, 1)))

/** The years the picker offers: a few back and ahead of now, and always the one shown. */
function years(shown: number, now: number): number[] {
  const first = Math.min(shown, now - 5)
  const last = Math.max(shown, now + 2)
  return Array.from({ length: last - first + 1 }, (_, i) => first + i)
}
const weekdayFormat = new Intl.DateTimeFormat(LOCALE, { weekday: 'short' })
const dayFormat = new Intl.DateTimeFormat(LOCALE, {
  weekday: 'long',
  day: 'numeric',
  month: 'long',
})

function dayTitle(day: Date, today: Date): string {
  const diff = Math.round((day.getTime() - today.getTime()) / (DAY * 1000))
  const names: Record<number, string> = { [-1]: 'Yesterday', 0: 'Today', 1: 'Tomorrow' }
  return names[diff] ? `${names[diff]} · ${dayFormat.format(day)}` : dayFormat.format(day)
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

/** What the chips' edge colours mean, in each mode. */
const LEGEND: Record<'watch' | 'manage', BadgeValue[]> = {
  watch: [
    { label: 'Ready', tone: 'ready' },
    { label: 'Upcoming', tone: 'upcoming' },
    { label: 'Not in library', tone: 'neutral' },
    { label: 'File offline', tone: 'missing' },
  ],
  manage: [
    { label: 'monitored', tone: 'available' },
    { label: 'wanted', tone: 'missing' },
    { label: 'unmonitored', tone: 'neutral' },
  ],
}

const name = (e: CalendarEntry) => e.series_title ?? e.title
const link = (e: CalendarEntry) =>
  e.series_id ? `/tv/${e.series_id}/${e.item_id}` : `/movies/${e.item_id}`
/** `S02E05`, `Release` for a movie, or null when the episode's number isn't known. */
function number(e: CalendarEntry): string | null {
  if (e.kind === 'movie') {
    return 'Release'
  }
  return e.number?.number == null ? null : episodeLabel(e.number)
}

const detail = (e: CalendarEntry) =>
  e.kind === 'movie' ? 'Release' : [number(e), e.title].filter(Boolean).join(' · ')

function go(month: Month, view: View) {
  const query = new URLSearchParams({ month: monthParam(month) })
  if (view === 'list') {
    query.set('view', 'list')
  }
  navigate(`/calendar?${query}`)
}

export function CalendarPage() {
  const [mode] = useMode()
  const manage = mode === 'manage'
  // Taken once, so every render agrees on which day is today.
  const [today] = useState(() => startOfDay(new Date()))
  const month = parseMonth(useQueryParam('month'), today)
  const view: View = useQueryParam('view') === 'list' ? 'list' : 'month'
  const [picked, setPicked] = useState<number | null>(null)

  const weeks = monthGrid(month, FIRST_DAY)
  const first = weeks[0][0]
  const last = weeks[weeks.length - 1][6]
  // A day either side, so dates without a time near the edges are still caught.
  const from = first.getTime() / 1000 - DAY
  const to = last.getTime() / 1000 + 2 * DAY
  const entries = useApi(`calendar:${from}:${to}`, () => api.calendar(from, to))

  const days = new Map<number, CalendarEntry[]>()
  for (const e of entries.data ?? []) {
    const key = airDay(e.airs_at, e.airs_time_known).getTime()
    days.set(key, [...(days.get(key) ?? []), e])
  }
  const inMonth = [...days.keys()]
    .filter((k) => sameMonth(new Date(k), month))
    .sort((a, b) => a - b)

  // The picked day while it's on the grid; else today, or the month's first busy day.
  const onGrid = (k: number | null) => k !== null && k >= first.getTime() && k <= last.getTime()
  const selected = onGrid(picked)
    ? picked
    : sameMonth(today, month)
      ? today.getTime()
      : (inMonth[0] ?? null)

  const current = sameMonth(today, month)
  const legend = LEGEND[manage ? 'manage' : 'watch']

  return (
    <div className={agenda.page}>
      <div className={agenda.inner}>
        <div className={agenda.header}>
          <div className={agenda.heading}>
            <h1>Calendar</h1>
            <p className={agenda.lead}>
              Times are yours; dates without a time come from the provider.
            </p>
          </div>
          <Segmented
            label="View"
            variant="box"
            value={view}
            onChange={(v) => go(month, v)}
            options={[
              { value: 'month', label: 'Month', icon: <GridIcon /> },
              { value: 'list', label: 'List', icon: <ListIcon /> },
            ]}
          />
        </div>

        <div className={styles.toolbar}>
          <div className={styles.picker}>
            <div className={styles.monthStep}>
              <button
                type="button"
                className={styles.step}
                aria-label="Previous month"
                title="Previous month"
                onClick={() => go(addMonths(month, -1), view)}
              >
                <ArrowLeftIcon />
              </button>
              <Dropdown
                label="Month"
                value={month.month}
                options={MONTHS.map((m, i) => ({ value: i, label: m }))}
                onChange={(m) => go({ year: month.year, month: m }, view)}
              />
              <button
                type="button"
                className={styles.step}
                aria-label="Next month"
                title="Next month"
                onClick={() => go(addMonths(month, 1), view)}
              >
                <ArrowRightIcon />
              </button>
            </div>
            <Dropdown
              label="Year"
              value={month.year}
              options={years(month.year, today.getFullYear()).map((y) => ({
                value: y,
                label: String(y),
              }))}
              onChange={(y) => go({ year: y, month: month.month }, view)}
            />
          </div>
          <button
            type="button"
            className={styles.todayButton}
            disabled={current}
            onClick={() => go(parseMonth(null, today), view)}
          >
            Today
          </button>
        </div>

        {entries.error && <Failed error={entries.error} onRetry={entries.reload} />}
        {!entries.error && !entries.data && <Loading />}

        {entries.data && view === 'month' && (
          <>
            <div className={styles.grid}>
              <div className={styles.weekdays} aria-hidden>
                {weeks[0].map((d) => (
                  <span key={d.getDay()}>{weekdayFormat.format(d)}</span>
                ))}
              </div>
              {weeks.map((week) => (
                <div key={week[0].getTime()} className={styles.week}>
                  {week.map((d) => {
                    const key = d.getTime()
                    const list = days.get(key) ?? []
                    const shown = list.length > CHIPS ? CHIPS - 1 : CHIPS
                    const classes = [
                      styles.cell,
                      sameMonth(d, month) ? '' : styles.outside,
                      key === today.getTime() ? styles.isToday : '',
                      key === selected ? styles.selected : '',
                      key < today.getTime() ? styles.past : '',
                    ]
                    return (
                      <div key={key} className={classes.join(' ')}>
                        <button
                          type="button"
                          className={styles.dayButton}
                          aria-pressed={key === selected}
                          aria-label={`${dayTitle(d, today)}${list.length ? `, ${list.length} airing` : ''}`}
                          onClick={() => setPicked(key)}
                        >
                          <span className={styles.dayNumber}>{d.getDate()}</span>
                        </button>
                        {list.length > 0 && (
                          <ul className={styles.chips}>
                            {list.slice(0, shown).map((e) => (
                              <li key={e.item_id}>
                                <Chip entry={e} badge={badge(e, manage)} />
                              </li>
                            ))}
                          </ul>
                        )}
                        {list.length > shown && (
                          <button
                            type="button"
                            className={styles.more}
                            onClick={() => setPicked(key)}
                          >
                            +{list.length - shown} more
                          </button>
                        )}
                        {list.length > 0 && (
                          <span className={styles.dots} aria-hidden>
                            {list.slice(0, 4).map((e) => (
                              <span
                                key={e.item_id}
                                className={`${styles.dot} ${styles[badge(e, manage).tone]}`}
                              />
                            ))}
                          </span>
                        )}
                      </div>
                    )
                  })}
                </div>
              ))}
            </div>
            <ul className={styles.legend} aria-label="Colours">
              {legend.map((b) => (
                <li key={b.label}>
                  <span className={`${styles.dot} ${styles[b.tone]}`} />
                  {b.label}
                </li>
              ))}
            </ul>
            {selected !== null && (
              <DayGroup
                day={new Date(selected)}
                today={today}
                list={days.get(selected) ?? []}
                manage={manage}
              />
            )}
          </>
        )}

        {entries.data && view === 'list' && (
          <>
            {inMonth.length === 0 && <div className={agenda.empty}>Nothing airs this month.</div>}
            {inMonth.map((key) => (
              <DayGroup
                key={key}
                day={new Date(key)}
                today={today}
                list={days.get(key) ?? []}
                manage={manage}
              />
            ))}
          </>
        )}
      </div>
    </div>
  )
}

/** A heading-sized native select, with a chevron. */
function Dropdown({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: number
  options: { value: number; label: string }[]
  onChange: (value: number) => void
}) {
  return (
    <span className={styles.dropdown}>
      <select aria-label={label} value={value} onChange={(e) => onChange(Number(e.target.value))}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <ChevronDownIcon />
    </span>
  )
}

function Chip({ entry: e, badge: b }: { entry: CalendarEntry; badge: BadgeValue }) {
  const sub = [e.airs_time_known ? time(e.airs_at) : null, number(e)].filter(Boolean).join(' · ')
  return (
    <Link
      to={link(e)}
      className={`${styles.chip} ${styles[b.tone]}`}
      style={{ '--tone': seriesTone(name(e)) } as CSSProperties}
      title={`${name(e)} · ${detail(e)} · ${b.label}`}
    >
      <span className={styles.chipName}>{name(e)}</span>
      {sub && <span className={styles.chipSub}>{sub}</span>}
    </Link>
  )
}

function DayGroup({
  day,
  today,
  list,
  manage,
}: {
  day: Date
  today: Date
  list: CalendarEntry[]
  manage: boolean
}) {
  const title = dayTitle(day, today)
  const isToday = day.getTime() === today.getTime()
  return (
    <section className={agenda.group} aria-label={title}>
      <h2 className={`${agenda.groupTitle} ${isToday ? agenda.today : ''}`}>{title}</h2>
      {list.length === 0 && <div className={styles.quiet}>Nothing airs this day.</div>}
      <ul className={agenda.list}>
        {list.map((e) => (
          <li key={e.item_id}>
            <Link to={link(e)} className={agenda.row}>
              <span className={agenda.tile} style={{ background: seriesTone(name(e)) }}>
                {e.airs_time_known ? time(e.airs_at) : <small>All day</small>}
              </span>
              <span className={agenda.text}>
                <span className={agenda.title}>{name(e)}</span>
                <span className={agenda.meta}>{detail(e)}</span>
              </span>
              <span className={agenda.badges}>
                <Badge badge={badge(e, manage)} small />
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  )
}
