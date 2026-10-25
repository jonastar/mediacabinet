import type { SeriesDetail } from '../../api/client'
import styles from './ManagePanels.module.css'

const NOT_BUILT = "This isn't built yet"

const PROVIDERS: Record<string, string> = {
  tmdb: 'TMDB',
  tvdb: 'TVDB',
  anidb: 'AniDB',
  imdb: 'IMDb',
  trakt: 'Trakt',
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
}

/** Monitoring, Metadata, Orderings and Location: Manage mode only. */
export function ManagePanels({ series, monitorPending, onToggleMonitor }: Props) {
  const primary = series.external_ids.find((x) => x.is_primary)
  const others = [
    ...new Set(series.external_ids.filter((x) => !x.is_primary).map((x) => provider(x.provider))),
  ]
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
          <select value={series.quality_profile_id ?? ''} disabled title={NOT_BUILT}>
            <option value="">None</option>
            {series.quality_profiles.map((q) => (
              <option key={q.id} value={q.id}>
                {q.name}
              </option>
            ))}
          </select>
        </label>
        <label className={styles.field}>
          Wanted follows
          <select disabled title={NOT_BUILT}>
            <option>All episodes</option>
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
        <label className={styles.check}>
          <input type="checkbox" disabled title={NOT_BUILT} />
          Lock my edits from refresh
        </label>
        <button type="button" className={styles.button} disabled title={NOT_BUILT}>
          Change match…
        </button>
      </section>

      <section aria-labelledby="p-ord" className={styles.panel}>
        <h2 id="p-ord">Orderings</h2>
        {series.views.map((v) => (
          <span key={v.id} className={styles.spread}>
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
        ))}
        <button type="button" className={styles.button} disabled title={NOT_BUILT}>
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
        <button type="button" className={styles.button} disabled title={NOT_BUILT}>
          Move series…
        </button>
      </section>
    </div>
  )
}
