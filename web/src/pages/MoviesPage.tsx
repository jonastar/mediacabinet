import { api } from '../api/client'
import { useApi } from '../api/useApi'
import { MoviePoster } from '../components/Poster'
import { Failed, Loading } from '../components/Status'
import { useMode } from '../lib/prefs'
import styles from './Page.module.css'

export function MoviesPage() {
  const [mode] = useMode()
  const movies = useApi('movies', api.movies)
  if (movies.error) {
    return <Failed error={movies.error} onRetry={movies.reload} />
  }
  if (!movies.data) {
    return <Loading />
  }
  return (
    <div className={styles.page}>
      <h1 className={styles.title}>Movies</h1>
      {movies.data.length === 0 ? (
        <p className={styles.muted}>No movies yet. Add a movie library in Libraries & roots.</p>
      ) : (
        <div className={styles.posterGrid}>
          {movies.data.map((m) => (
            <MoviePoster key={m.id} movie={m} mode={mode} />
          ))}
        </div>
      )}
    </div>
  )
}
