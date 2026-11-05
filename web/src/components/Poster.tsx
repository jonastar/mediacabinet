import { artworkUrl, type MovieSummary, type SeriesSummary } from '../api/client'
import { CoverImage } from './Artwork'
import { airWeekday, runtime, seriesTone } from '../lib/format'
import type { Mode } from '../lib/prefs'
import { itemPath } from '../lib/router'
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
    return `New season ${airWeekday(next.airs_at, next.airs_time_known)}`
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
    <Link to={itemPath('series', s.id, s.library_id)} className={styles.poster}>
      <div className={styles.art} style={{ background: seriesTone(s.title) }}>
        <CoverImage src={artworkUrl(s, 'poster')} alt={s.title} />
        {badge && <span className={styles.badge}>{badge}</span>}
        {!s.artwork.includes('poster') && <span className={styles.title}>{s.title}</span>}
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

export function MoviePoster({ movie: m, mode }: { movie: MovieSummary; mode: Mode }) {
  const line =
    mode === 'manage'
      ? [m.availability.replace('_', ' '), m.versions > 1 ? `${m.versions} versions` : null]
          .filter(Boolean)
          .join(' · ')
      : [m.year, runtime(m.runtime_seconds)].filter(Boolean).join(' · ')
  const available = m.availability === 'available'
  return (
    <Link to={itemPath('movie', m.id, m.library_id)} className={styles.poster}>
      <div className={styles.art} style={{ background: seriesTone(m.title) }}>
        <CoverImage src={artworkUrl(m, 'poster') ?? artworkUrl(m, 'thumbnail')} alt={m.title} />
        {m.watch?.played && <span className={styles.badge}>Watched</span>}
        {!m.artwork.includes('poster') && <span className={styles.title}>{m.title}</span>}
      </div>
      {mode === 'manage' && (
        <div className={styles.bar} aria-hidden="true">
          <div
            style={{
              width: '100%',
              background: available
                ? 'var(--state-available)'
                : m.availability === 'missing'
                  ? 'var(--state-missing)'
                  : 'transparent',
            }}
          />
        </div>
      )}
      <span className={styles.line}>{line || '\u00a0'}</span>
    </Link>
  )
}
