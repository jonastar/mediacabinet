import { api } from '../api/client'
import { useApi } from '../api/useApi'
import { Poster } from '../components/Poster'
import { Failed, Loading } from '../components/Status'
import { useMode } from '../lib/prefs'
import styles from './Page.module.css'

export function ShowsPage() {
  const [mode] = useMode()
  const shows = useApi('series', api.series)
  if (shows.error) {
    return <Failed error={shows.error} onRetry={shows.reload} />
  }
  if (!shows.data) {
    return <Loading />
  }
  return (
    <div className={styles.page}>
      <h1 className={styles.title}>TV Shows</h1>
      {shows.data.length === 0 ? (
        <p className={styles.muted}>No shows yet.</p>
      ) : (
        <div className={styles.posterGrid}>
          {shows.data.map((s) => (
            <Poster key={s.id} series={s} mode={mode} />
          ))}
        </div>
      )}
    </div>
  )
}
