import { useState } from 'react'
import { api } from '../api/client'
import { useApi } from '../api/useApi'
import { Link } from '../components/Link'
import { MoviePoster, Poster } from '../components/Poster'
import { Failed, Loading } from '../components/Status'
import { episodeLabel } from '../lib/format'
import { useMode } from '../lib/prefs'
import { navigate, useQueryParam } from '../lib/router'
import styles from './Page.module.css'

export function SearchPage() {
  const [mode] = useMode()
  const q = useQueryParam('q') ?? ''
  const [draft, setDraft] = useState(q)
  const results = useApi(q ? `search:${q}` : null, () => api.search(q))
  const r = results.data
  const empty = r && r.series.length + r.movies.length + r.episodes.length === 0

  return (
    <div className={styles.page}>
      <h1 className={styles.title}>Search</h1>
      <form
        role="search"
        onSubmit={(e) => {
          e.preventDefault()
          navigate(`/search?q=${encodeURIComponent(draft.trim())}`)
        }}
      >
        <label htmlFor="search-q" className="visually-hidden">
          Search
        </label>
        <input
          id="search-q"
          type="search"
          className={styles.searchInput}
          value={draft}
          placeholder="Shows, movies, episodes…"
          onChange={(e) => setDraft(e.target.value)}
        />
      </form>
      {results.error && <Failed error={results.error} onRetry={results.reload} />}
      {q && !r && !results.error && <Loading label="Searching…" />}
      {empty && <p className={styles.muted}>Nothing in the library matches “{q}”.</p>}
      {r && r.series.length > 0 && (
        <section className={styles.section}>
          <h2>TV Shows</h2>
          <div className={styles.posterGrid}>
            {r.series.map((s) => (
              <Poster key={s.id} series={s} mode={mode} />
            ))}
          </div>
        </section>
      )}
      {r && r.movies.length > 0 && (
        <section className={styles.section}>
          <h2>Movies</h2>
          <div className={styles.posterGrid}>
            {r.movies.map((m) => (
              <MoviePoster key={m.id} movie={m} mode={mode} />
            ))}
          </div>
        </section>
      )}
      {r && r.episodes.length > 0 && (
        <section className={styles.section}>
          <h2>Episodes</h2>
          <ul className={styles.hits}>
            {r.episodes.map((e) => (
              <li key={e.item_id}>
                <Link to={`/tv/${e.series_id}/${e.item_id}`} className={styles.hit}>
                  <b>{e.title}</b>
                  <span className={styles.muted}>
                    {e.series_title} · {episodeLabel(e.number)}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  )
}
