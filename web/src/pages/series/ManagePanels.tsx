import { useState } from 'react'
import { api, type SeriesDetail } from '../../api/client'
import { ago } from '../../lib/format'
import { SearchPicker } from '../review/Pickers'
import { LocationActions } from './LocationActions'
import styles from './ManagePanels.module.css'

const LOCKABLE = ['title', 'year', 'overview']

const PROVIDERS: Record<string, string> = {
  tmdb: 'TMDB',
  tvdb: 'TVDB',
  anidb: 'AniDB',
  imdb: 'IMDb',
  trakt: 'Trakt',
  computed: 'derived',
}
const provider = (p: string) => PROVIDERS[p] ?? p

const POLICIES: Record<string, string> = {
  managed: 'managed root',
  adaptive: 'adaptive root',
  read_only: 'read-only root',
}

interface Props {
  series: SeriesDetail
  monitorPending: boolean
  onToggleMonitor: () => void
  /** Reload after a change here. */
  onChanged: () => Promise<unknown>
  onNewPlaylist: () => void
}

/** Monitoring, Metadata, Orderings and Location: Manage mode only. */
export function ManagePanels({
  series,
  monitorPending,
  onToggleMonitor,
  onChanged,
  onNewPlaylist,
}: Props) {
  const [changingMatch, setChangingMatch] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const primary = series.external_ids.find((x) => x.is_primary)
  const others = [
    ...new Set(series.external_ids.filter((x) => !x.is_primary).map((x) => provider(x.provider))),
  ]
  const locked = series.locked_fields.length > 0

  const act = async (action: () => Promise<unknown>) => {
    setError(null)
    try {
      await action()
      await onChanged()
    } catch (e) {
      setError((e as Error).message)
    }
  }

  return (
    <div className={styles.panels}>
      <section aria-labelledby="p-mon" className={styles.panel}>
        <h2 id="p-mon">Monitoring</h2>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={monitorPending ? !series.monitored : series.monitored}
            onChange={onToggleMonitor}
          />
          Monitor new episodes
        </label>
        <label className={styles.field}>
          Quality profile
          <select
            value={series.quality_profile_id ?? ''}
            onChange={(e) =>
              act(() =>
                api.setMonitoring(series.id, {
                  quality_profile_id: e.target.value ? Number(e.target.value) : null,
                }),
              )
            }
          >
            <option value="">None: any file will do</option>
            {series.quality_profiles.map((q) => (
              <option key={q.id} value={q.id}>
                {q.name}
              </option>
            ))}
          </select>
        </label>
        <label className={styles.field}>
          Wanted follows
          <select
            value={series.wanted_follows ?? ''}
            onChange={(e) =>
              act(() => api.setMonitoring(series.id, { wanted_follows: e.target.value || null }))
            }
          >
            <option value="">All episodes</option>
            {series.views
              .filter((v) => v.kind === 'playlist')
              .map((v) => (
                <option key={v.id} value={v.id}>
                  {v.name} playlist
                </option>
              ))}
          </select>
        </label>
      </section>

      <section aria-labelledby="p-meta" className={styles.panel}>
        <h2 id="p-meta">Metadata</h2>
        {primary ? (
          <span className={styles.line}>
            Primary: <b>{provider(primary.provider)}</b>{' '}
            <span className={styles.id}>
              {primary.provider}:{primary.value}
            </span>
          </span>
        ) : (
          <span className={styles.muted}>Not matched to a provider</span>
        )}
        {others.length > 0 && (
          <span className={styles.muted}>Also linked: {others.join(', ')}</span>
        )}
        {series.refresh_error ? (
          <span className={styles.warning}>Last refresh failed: {series.refresh_error}</span>
        ) : (
          series.refreshed_at && (
            <span className={styles.muted}>Refreshed {ago(series.refreshed_at)}</span>
          )
        )}
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={locked}
            onChange={() => act(() => api.setLocks(series.id, locked ? [] : LOCKABLE))}
          />
          Lock my edits from refresh
        </label>
        {locked && <span className={styles.muted}>Locked: {series.locked_fields.join(', ')}</span>}
        <button
          type="button"
          className={styles.button}
          onClick={() => setChangingMatch(!changingMatch)}
        >
          {changingMatch ? 'Cancel' : 'Change match…'}
        </button>
        {changingMatch && (
          <SearchPicker
            kind="tv"
            initial={series.title}
            year={series.year}
            onPick={(r) =>
              act(async () => {
                await api.rematchSeries(series.id, r.id)
                setChangingMatch(false)
              })
            }
          />
        )}
      </section>

      <section aria-labelledby="p-ord" className={styles.panel}>
        <h2 id="p-ord">Orderings</h2>
        {series.views.map((v) => {
          const matching = series.match_view === v.id || (!series.match_view && v.is_default)
          return (
            <div key={v.id} className={styles.ordering}>
              <span className={styles.spread}>
                <span>
                  {v.name} · {v.kind === 'playlist' ? 'playlist' : provider(v.source)}
                </span>
                {v.is_default ? (
                  <span className={styles.default}>Default</span>
                ) : (
                  v.kind === 'playlist' && (
                    <span className={styles.note}>{v.shared ? 'Shared' : 'Yours'}</span>
                  )
                )}
              </span>
              {v.kind !== 'playlist' &&
                (matching ? (
                  <span className={styles.note}>Used for file matching</span>
                ) : (
                  <button
                    type="button"
                    className={styles.link}
                    onClick={() =>
                      act(() => api.setMatchOrdering(series.id, v.is_default ? null : v.id))
                    }
                  >
                    Use for file matching
                  </button>
                ))}
            </div>
          )
        })}
        <button type="button" className={styles.button} onClick={onNewPlaylist}>
          New playlist…
        </button>
      </section>

      <section aria-labelledby="p-loc" className={styles.panel}>
        <h2 id="p-loc">Location</h2>
        {series.locations.length === 0 && <span className={styles.muted}>No folder yet</span>}
        {series.locations.map((l) => (
          <div
            key={`${l.root_id}:${l.path}`}
            style={{ display: 'flex', flexDirection: 'column', gap: 4 }}
          >
            <span className={`${styles.mono} ${l.online ? '' : styles.offline}`}>
              {`${l.root_path.replace(/\/$/, '')}/${l.path}`}
            </span>
            <span className={styles.muted}>
              {[
                l.is_primary ? 'Primary' : null,
                l.online ? POLICIES[l.write_policy] : 'Offline',
                l.online && !l.is_primary
                  ? null
                  : `${l.file_count} ${l.file_count === 1 ? 'file' : 'files'}`,
              ]
                .filter(Boolean)
                .join(' · ')}
            </span>
          </div>
        ))}
        <LocationActions series={series} onDone={onChanged} />
      </section>
      {error && (
        <p className={styles.warning} role="alert" style={{ gridColumn: '1 / -1' }}>
          {error}
        </p>
      )}
    </div>
  )
}
