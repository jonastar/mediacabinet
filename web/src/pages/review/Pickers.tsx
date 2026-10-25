import { useState } from 'react'
import { api, type LibraryKind, type SearchResult } from '../../api/client'
import { useApi } from '../../api/useApi'
import { episodeLabel } from '../../lib/format'
import styles from './Review.module.css'

/** Search the metadata provider and pick a series or movie. */
export function SearchPicker({
  kind,
  initial,
  year,
  onPick,
}: {
  kind: LibraryKind
  initial: string
  year?: number | null
  onPick: (r: SearchResult) => void
}) {
  const [query, setQuery] = useState(initial)
  const [yearText, setYearText] = useState(year ? String(year) : '')
  const [results, setResults] = useState<SearchResult[] | null>(null)
  const [error, setError] = useState<string | null>(null)

  const search = async () => {
    setError(null)
    try {
      setResults(await api.searchMetadata(kind, query, yearText ? Number(yearText) : null))
    } catch (e) {
      setError((e as Error).message)
    }
  }

  return (
    <div className={styles.picker}>
      <div className={styles.row}>
        <input
          className={styles.input}
          aria-label="Title"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && query && search()}
        />
        <input
          className={`${styles.input} ${styles.year}`}
          aria-label="Year"
          placeholder="Year"
          inputMode="numeric"
          value={yearText}
          onChange={(e) => setYearText(e.target.value.replace(/\D/g, '').slice(0, 4))}
        />
        <button type="button" className={styles.button} disabled={!query} onClick={search}>
          Search
        </button>
      </div>
      {error && <p className={styles.error}>{error}</p>}
      {results && results.length === 0 && <p className={styles.muted}>Nothing found.</p>}
      {results && results.length > 0 && (
        <ul className={styles.results}>
          {results.map((r) => (
            <li key={r.id}>
              <button type="button" className={styles.result} onClick={() => onPick(r)}>
                <b>
                  {r.title}
                  {r.year ? ` (${r.year})` : ''}
                </b>
                {r.overview && <span className={styles.muted}>{r.overview}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** Pick episodes of a series (several for a multi-episode file). */
export function EpisodePicker({
  seriesId,
  onPick,
}: {
  seriesId: number
  onPick: (ids: number[]) => void
}) {
  const list = useApi(`review-episodes:${seriesId}`, () => api.episodes(seriesId))
  const [chosen, setChosen] = useState<number[]>([])
  if (!list.data) {
    return <p className={styles.muted}>{list.error ? list.error.message : 'Loading episodes…'}</p>
  }
  return (
    <div className={styles.picker}>
      <label className={styles.muted} htmlFor="episode-pick">
        Episodes (hold Ctrl or ⌘ to choose several)
      </label>
      <select
        id="episode-pick"
        multiple
        size={8}
        className={styles.input}
        value={chosen.map(String)}
        onChange={(e) => setChosen(Array.from(e.target.selectedOptions, (o) => Number(o.value)))}
      >
        {list.data.entries.map((e) => (
          <option key={e.item_id} value={e.item_id}>
            {episodeLabel(e.number)} · {e.title}
          </option>
        ))}
      </select>
      <div className={styles.row}>
        <button
          type="button"
          className={`${styles.button} ${styles.primary}`}
          disabled={chosen.length === 0}
          onClick={() => onPick(chosen)}
        >
          Link and pin
        </button>
      </div>
    </div>
  )
}
