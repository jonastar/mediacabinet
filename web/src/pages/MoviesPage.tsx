import { api, type Library } from '../api/client'
import { useApi } from '../api/useApi'
import { MoviePoster } from '../components/Poster'
import { Failed, Loading } from '../components/Status'
import { useMode } from '../lib/prefs'
import styles from './Page.module.css'

/** All movies, or with `library` the movies in it. */
export function MoviesPage({ library }: { library?: Library }) {
  const [mode] = useMode()
  const movies = useApi(`movies:${library?.id ?? 'all'}`, () => api.movies(library?.id))
  if (movies.error) {
    return <Failed error={movies.error} onRetry={movies.reload} />
  }
  if (!movies.data) {
    return <Loading />
  }
  return (
    <div className={styles.page}>
      <h1 className={styles.title}>{library?.name ?? 'Movies'}</h1>
      {movies.data.length === 0 ? (
        <p className={styles.muted}>
          {library
            ? 'No movies in this library yet.'
            : 'No movies yet. Add a movie library in Libraries & roots.'}
        </p>
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
