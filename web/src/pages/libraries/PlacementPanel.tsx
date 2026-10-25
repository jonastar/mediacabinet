import { api } from '../../api/client'
import { useApi } from '../../api/useApi'
import styles from './Libraries.module.css'

const LABELS: Record<string, string> = {
  managed: 'Managed',
  adaptive: 'Adaptive',
  read_only: 'Read-only',
}

/** "Where does a new file go?", for the next episode a series here is missing. */
export function PlacementPanel({ libraryId, version }: { libraryId: number; version: string }) {
  const example = useApi(`placement:${libraryId}:${version}`, () => api.placementExample(libraryId))
  const e = example.data
  if (!e) {
    return null
  }
  return (
    <section aria-labelledby="placement" className={styles.panel}>
      <h2 id="placement">Where does a new file go?</h2>
      <p className={styles.muted}>
        Example:{' '}
        <b style={{ color: 'var(--text)' }}>
          {e.series_title} {e.episode_label} · {e.episode_title}
        </b>{' '}
        arrives from an import as <span className={styles.mono}>{e.filename}</span>.{' '}
        {e.series_title} already has a stored location in{' '}
        <span className={styles.mono}>{e.root_path}</span>.
      </p>
      <div className={styles.outcomes}>
        {e.outcomes.map((o) => (
          <div key={o.policy} className={styles.outcome}>
            <span className={styles.outcomePolicy}>{LABELS[o.policy] ?? o.policy}</span>
            <span className={styles.outcomeBody}>
              <span className={styles.outcomePath}>
                {o.path ? `${e.root_path.replace(/\/$/, '')}/${o.path}` : '— not written here'}
              </span>
              <span className={styles.muted} style={{ fontSize: 13 }}>
                {o.note}
              </span>
            </span>
          </div>
        ))}
      </div>
    </section>
  )
}
