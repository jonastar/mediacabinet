import { api, type Library } from '../api/client'
import { useApi } from '../api/useApi'
import { Poster } from '../components/Poster'
import { Failed, Loading } from '../components/Status'
import { useMode } from '../lib/prefs'
import styles from './Page.module.css'

/** All series, or with `library` the series in it. */
export function ShowsPage({ library }: { library?: Library }) {
  const [mode] = useMode()
  const shows = useApi(`series:${library?.id ?? 'all'}`, () => api.series(library?.id))
  if (shows.error) {
    return <Failed error={shows.error} onRetry={shows.reload} />
  }
  if (!shows.data) {
    return <Loading />
  }
  return (
    <div className={styles.page}>
      <h1 className={styles.title}>{library?.name ?? 'TV Shows'}</h1>
      {shows.data.length === 0 ? (
        <p className={styles.muted}>
          {library ? 'No shows in this library yet.' : 'No shows yet.'}
        </p>
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
