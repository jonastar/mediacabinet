import type { SeriesSummary } from '../api/client'
import { seriesTone, weekday } from '../lib/format'
import type { Mode } from '../lib/prefs'
import { Link } from './Link'
import styles from './Poster.module.css'

const WEEK = 7 * 24 * 3600

/** "1 new", or "New season Wed" when a season premiere airs this week. */
function posterBadge(s: SeriesSummary, now = Date.now() / 1000): string | null {
  const next = s.next_airing
  if (
    next?.airs_at &&
    next.airs_at - now < WEEK &&
    next.number?.number === 1 &&
    next.number.season
  ) {
    return `New season ${weekday(next.airs_at)}`
  }
  return s.new_episodes > 0 ? `${s.new_episodes} new` : null
}

export function Poster({ series: s, mode }: { series: SeriesSummary; mode: Mode }) {
  const c = s.counts
  const badge = posterBadge(s)
  const line =
    mode === 'manage'
      ? `${c.available} of ${c.total}${c.missing ? ` · ${c.missing} missing` : ''}`
      : c.available === c.total
        ? `${c.total} episodes`
        : `${c.available} episodes ready`
  const width = (n: number) => `${c.total ? (n / c.total) * 100 : 0}%`
  return (
    <Link to={`/tv/${s.id}`} className={styles.poster}>
      <div className={styles.art} style={{ background: seriesTone(s.title) }}>
        {badge && <span className={styles.badge}>{badge}</span>}
        <span className={styles.title}>{s.title}</span>
      </div>
      {mode === 'manage' && (
        <div className={styles.bar} aria-hidden="true">
          <div style={{ width: width(c.available), background: 'var(--state-available)' }} />
          <div style={{ width: width(c.missing), background: 'var(--state-missing)' }} />
        </div>
      )}
      <span className={styles.line}>{line}</span>
    </Link>
  )
}
