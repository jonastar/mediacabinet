import { useState } from 'react'
import { api, artworkUrl, type LinkedFile } from '../api/client'
import { useApi } from '../api/useApi'
import { EditDetails } from '../components/EditDetails'
import { PlayIcon } from '../components/Icons'
import { LibraryBack } from '../components/LibraryBack'
import { itemPath, useCanonicalPath } from '../lib/router'
import { DetailsToggle } from '../components/DetailsToggle'
import { Link } from '../components/Link'
import { Failed, Loading } from '../components/Status'
import { heroBackground } from '../lib/artwork'
import { ago, bytes, filesSummary, matchSummary, resolution, seriesTone } from '../lib/format'
import { useMode } from '../lib/prefs'
import styles from './SeriesPage.module.css'

function runtime(seconds: number | null | undefined) {
  if (!seconds) {
    return null
  }
  const m = Math.round(seconds / 60)
  return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`
}

function versionLabel(f: LinkedFile) {
  return [
    f.edition,
    resolution(f.height),
    f.hdr_format?.toUpperCase(),
    f.part ? `part ${f.part}` : null,
  ]
    .filter(Boolean)
    .join(' · ')
}

export function MoviePage({ id }: { id: number }) {
  const [mode] = useMode()
  const movie = useApi(`movie:${id}`, () => api.movieDetail(id))
  useCanonicalPath(movie.data ? itemPath('movie', id, movie.data.library_id) : null)
  const [editing, setEditing] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  if (movie.error) {
    return <Failed error={movie.error} onRetry={movie.reload} />
  }
  if (!movie.data) {
    return <Loading />
  }
  const m = movie.data
  const manage = mode === 'manage'
  const tone = seriesTone(m.title)
  const background = heroBackground(
    artworkUrl(m, 'backdrop'),
    manage ? `color-mix(in srgb, ${tone} 70%, var(--bg))` : tone,
  )
  const meta = [m.year, m.genres[0], runtime(m.runtime_seconds)].filter(Boolean).join(' · ')
  const present = m.files.filter((f) => f.status === 'present')

  const refresh = async () => {
    setNotice('Refreshing…')
    try {
      await api.refreshItem(id)
      setNotice('Refreshed.')
      await movie.reload()
    } catch (e) {
      setNotice(`Refresh failed: ${(e as Error).message}`)
    }
  }

  return (
    <>
      <section
        aria-labelledby="movie-title"
        className={`${styles.hero} ${manage ? styles.manage : ''}`}
        style={{ background }}
      >
        <div className={styles.heroBar}>
          <LibraryBack
            libraryId={m.library_id}
            fallback={{ to: '/movies', label: 'Movies' }}
            className={styles.back}
          />
          <DetailsToggle />
        </div>
        <div className={styles.heroBody}>
          {manage && <span className={styles.managing}>Managing · changes apply for everyone</span>}
          <h1 id="movie-title" className={styles.title}>
            {m.title}
          </h1>
          {meta && <p className={styles.heroMeta}>{meta}</p>}
          {!manage && m.overview && <p className={styles.overview}>{m.overview}</p>}
          {!manage && (
            <div className={styles.actions}>
              {present.length > 0 ? (
                <>
                  <Link to={`/play/${m.id}`} className={styles.primary}>
                    <PlayIcon />
                    {(m.watch?.position_seconds ?? 0) > 0 && !m.watch?.played ? 'Resume' : 'Play'}
                  </Link>
                  <button
                    type="button"
                    className={styles.secondary}
                    aria-pressed={m.watch?.played ?? false}
                    onClick={() =>
                      api.setPlayed(m.id, !(m.watch?.played ?? false)).then(movie.reload)
                    }
                  >
                    {m.watch?.played ? 'Watched ✓' : 'Mark watched'}
                  </button>
                </>
              ) : (
                <span className={styles.heroMeta}>
                  {m.availability === 'missing' ? 'File offline' : 'Not in library'}
                </span>
              )}
              {present.length > 1 && (
                <span className={styles.heroMeta}>{filesSummary(present)}</span>
              )}
            </div>
          )}
          {manage && (
            <>
              <div className={styles.manageActions}>
                <button type="button" onClick={() => setEditing(!editing)}>
                  Edit details
                </button>
                <button type="button" onClick={refresh}>
                  Refresh metadata
                </button>
              </div>
              {notice && (
                <p className={styles.notice} role="status">
                  {notice}
                </p>
              )}
            </>
          )}
        </div>
      </section>

      <div className={styles.content}>
        {manage && editing && (
          <EditDetails
            itemId={id}
            title={m.title}
            year={m.year}
            overview={m.overview}
            onSaved={() => {
              setEditing(false)
              movie.reload()
            }}
            onCancel={() => setEditing(false)}
          />
        )}
        <div className={styles.toolbar}>
          <h2>{manage ? 'Files' : 'Versions'}</h2>
        </div>
        {m.files.length === 0 && <p className={styles.viewNote}>No files linked yet.</p>}
        <ul
          style={{
            listStyle: 'none',
            margin: 0,
            padding: 0,
            display: 'flex',
            flexDirection: 'column',
            gap: 10,
          }}
        >
          {m.files.map((f) => (
            <li
              key={f.file_id}
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: 4,
                padding: '14px 16px',
                borderRadius: 14,
                background: 'var(--surface)',
                border: '1px solid var(--border)',
              }}
            >
              <b>{versionLabel(f) || 'Main version'}</b>
              <span
                style={{
                  fontSize: 13.5,
                  color: f.status === 'present' ? 'var(--text-muted)' : 'var(--state-missing-text)',
                }}
              >
                {f.status === 'present'
                  ? bytes(f.size)
                  : `File ${f.status} · last seen ${ago(f.last_seen)}`}
              </span>
              {manage && (
                <span
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: 12.5,
                    color: 'var(--text-secondary)',
                    overflowWrap: 'anywhere',
                  }}
                >
                  {f.root_path}/{f.relative_path} · {matchSummary([f])}
                </span>
              )}
            </li>
          ))}
        </ul>
        {manage && m.refresh_error && (
          <p className={styles.notice}>Last refresh failed: {m.refresh_error}</p>
        )}
      </div>
    </>
  )
}
