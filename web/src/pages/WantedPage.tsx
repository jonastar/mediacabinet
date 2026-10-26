import { useState } from 'react'
import { api, type WantedItem, type WantedQuery } from '../api/client'
import { useApi } from '../api/useApi'
import { Badge } from '../components/Badge'
import { Link } from '../components/Link'
import { Segmented } from '../components/Segmented'
import { Failed, Loading } from '../components/Status'
import { airDate, ago, episodeLabel, resolution, seriesTone } from '../lib/format'
import type { Badge as BadgeValue } from '../lib/wording'
import { useLiveEvents } from '../lib/live'
import styles from './Agenda.module.css'

type Reason = 'all' | NonNullable<WantedQuery['reason']>
const PAGE = 100

const REASON_BADGE: Record<string, BadgeValue> = {
  missing: { label: 'missing', tone: 'missing' },
  not_acquired: { label: 'not acquired', tone: 'neutral' },
  upgrade: { label: 'upgrade', tone: 'upcoming' },
}

function statusBadge(i: WantedItem): BadgeValue | null {
  const s = i.statuses.find((x) => !x.stale)
  if (!s) {
    return null
  }
  const p = (s.detail as { progress?: number } | null)?.progress
  const label = `${s.source} · ${s.status}${typeof p === 'number' ? ` ${Math.round(p * 100)}%` : ''}`
  return { label, tone: s.status === 'failed' ? 'missing' : 'upcoming' }
}

function quality(i: WantedItem) {
  if (i.reason === 'upgrade') {
    return `${resolution(i.best_height) ?? 'unknown'} → ${resolution(i.cutoff_height) ?? '?'}${i.quality_profile ? ` (${i.quality_profile})` : ''}`
  }
  return i.quality_profile ? `target ${i.quality_profile}` : 'any quality'
}

/** Consecutive items of the same series or movie, keeping the list's order. */
function groups(items: WantedItem[]) {
  const out: { key: string; title: string; link: string; items: WantedItem[] }[] = []
  for (const i of items) {
    const key = i.series_id ? `s${i.series_id}` : `m${i.item_id}`
    const last = out[out.length - 1]
    if (last?.key === key) {
      last.items.push(i)
    } else {
      out.push({
        key,
        title: i.series_title ?? i.title,
        link: i.series_id ? `/tv/${i.series_id}` : `/movies/${i.item_id}`,
        items: [i],
      })
    }
  }
  return out
}

export function WantedPage() {
  const [reason, setReason] = useState<Reason>('all')
  const [shown, setShown] = useState(PAGE)
  const query = { reason: reason === 'all' ? undefined : reason, limit: shown }
  const page = useApi(`wanted:${reason}:${shown}`, () => api.wanted(query))
  useLiveEvents(['item.wanted', 'item.unwanted', 'status.changed'], page.reload)

  if (page.error) {
    return <Failed error={page.error} onRetry={page.reload} />
  }
  const data = page.data
  const inProgress = data?.items.filter((i) => statusBadge(i)).length ?? 0

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <div className={styles.header}>
          <div className={styles.heading}>
            <h1>Wanted</h1>
            <p className={styles.lead}>
              {data
                ? `${data.total} monitored ${data.total === 1 ? 'item' : 'items'} not in the library, or below their quality cutoff${inProgress ? ` · ${inProgress} in progress` : ''}.`
                : 'Monitored items not in the library, or below their quality cutoff.'}
            </p>
          </div>
          <Segmented
            label="Reason"
            variant="pills"
            value={reason}
            onChange={(r) => {
              setReason(r)
              setShown(PAGE)
            }}
            options={[
              { value: 'all', label: 'All' },
              { value: 'missing', label: 'Missing' },
              { value: 'not_acquired', label: 'Not acquired' },
              { value: 'upgrade', label: 'Upgrades' },
            ]}
          />
        </div>
        {!data && <Loading />}
        {data && data.items.length === 0 && (
          <div className={styles.empty}>
            Nothing wanted. Monitor a series or movie to have its missing episodes show up here.
          </div>
        )}
        {data &&
          groups(data.items).map((g) => (
            <section key={g.key} className={styles.group} aria-label={g.title}>
              <h2 className={styles.groupTitle}>
                <Link to={g.link}>{g.title}</Link>
              </h2>
              <ul className={styles.list}>
                {g.items.map((i) => {
                  const status = statusBadge(i)
                  return (
                    <li key={i.item_id}>
                      <Link to={g.link} className={styles.row}>
                        <span className={styles.tile} style={{ background: seriesTone(g.title) }}>
                          {i.kind === 'movie' ? <small>Movie</small> : episodeLabel(i.number)}
                        </span>
                        <span className={styles.text}>
                          <span className={styles.title}>{i.title}</span>
                          <span className={styles.meta}>
                            {airDate(i.airs_at, i.airs_time_known)}
                          </span>
                          <span className={styles.tech}>
                            {quality(i)}
                            {i.statuses.some((s) => s.stale) && !status
                              ? ` · last report ${ago(i.statuses[0].updated_at)} (stale)`
                              : ''}
                          </span>
                        </span>
                        <span className={styles.badges}>
                          {status && <Badge badge={status} small />}
                          <Badge badge={REASON_BADGE[i.reason]} small />
                        </span>
                      </Link>
                    </li>
                  )
                })}
              </ul>
            </section>
          ))}
        {data && data.total > data.items.length && (
          <button type="button" className={styles.more} onClick={() => setShown(shown + PAGE)}>
            Show more ({data.total - data.items.length} left)
          </button>
        )}
      </div>
    </div>
  )
}
