import { useState } from 'react'
import { api, type Library } from '../../api/client'
import { BackIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { Failed, Loading } from '../../components/Status'
import { navigate, useQueryParam } from '../../lib/router'
import { NotFoundPage } from '../NotFoundPage'
import { AddFolder } from './AddFolder'
import { PlacementPanel } from './PlacementPanel'
import { librarySummary, refreshStatus } from './roots'
import { RootRow } from './RootRow'
import { useLibraries } from './useLibraries'
import styles from './Libraries.module.css'

/** Without an import target, nothing new can be imported or organised into the library. */
function NoImportTarget({ library }: { library: Library }) {
  const things = library.kind === 'movies' ? 'movies' : 'shows'
  const writable = library.roots.some((r) => r.write_policy !== 'read_only')
  return (
    <div className={`${styles.panel} ${styles.warning}`} role="alert">
      <strong>
        {library.name} has no import target, so it can’t import new {things}.
      </strong>
      <p className={styles.muted}>
        {writable
          ? `Open a folder below and choose Make import target: new ${things} and their files go there.`
          : `Every folder here is read-only. Add a folder Media Cabinet may write to and make it the import target.`}
      </p>
    </div>
  )
}

/** Fetching the library's metadata again, beyond the schedule. */
function Metadata({ library, onChanged }: { library: Library; onChanged: () => void }) {
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const status = refreshStatus(library)

  const refresh = async () => {
    setBusy(true)
    setError(null)
    try {
      await api.refreshLibrary(library.id)
      onChanged()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const things = library.kind === 'movies' ? 'movie' : 'show'
  return (
    <section aria-labelledby="library-metadata" className={styles.panel}>
      <h2 id="library-metadata">Metadata</h2>
      <p className={styles.muted}>
        Details, episodes, artwork, cast and ratings refresh on a schedule: continuing shows daily,
        everything else weekly. Refresh now to fetch every {things} in {library.name} again. Locked
        fields are kept.
      </p>
      <span className={`${styles.status} ${styles[status.tone]}`} role="status">
        <span className={styles.dot} aria-hidden="true" />
        {status.text}
      </span>
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        <button
          type="button"
          className={`${styles.button} ${styles.small}`}
          disabled={busy || library.refreshing?.queued_again}
          onClick={refresh}
        >
          {library.refreshing ? 'Refresh again after this' : 'Refresh metadata'}
        </button>
      </div>
    </section>
  )
}

/** Video frames in place of artwork, for what has none. */
function Thumbnails({ library }: { library: Library }) {
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [asked, setAsked] = useState(false)

  const generate = async () => {
    setBusy(true)
    setError(null)
    try {
      await api.generateThumbnails(library.id)
      setAsked(true)
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-labelledby="library-thumbnails" className={styles.panel}>
      <h2 id="library-thumbnails">Thumbnails</h2>
      <p className={styles.muted}>
        Videos with no artwork show a frame from the video. Scans make these for new files, waiting
        while anyone is watching a converted stream. Generate now to also try again the files that
        failed.
      </p>
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        <button
          type="button"
          className={`${styles.button} ${styles.small}`}
          disabled={busy}
          onClick={generate}
        >
          Generate thumbnails
        </button>
        {asked && <Link to="/manage/tasks">See how it’s going in Tasks</Link>}
      </div>
    </section>
  )
}

function Settings({ library, onChanged }: { library: Library; onChanged: () => void }) {
  const [name, setName] = useState(library.name)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const act = async (action: () => Promise<unknown>) => {
    setBusy(true)
    setError(null)
    try {
      await action()
      return true
    } catch (e) {
      setError((e as Error).message)
      return false
    } finally {
      setBusy(false)
    }
  }

  const rename = async () => {
    if (await act(() => api.renameLibrary(library.id, name.trim()))) {
      onChanged()
    }
  }

  const remove = async () => {
    if (!window.confirm(`Delete the library ${library.name}?`)) {
      return
    }
    if (await act(() => api.deleteLibrary(library.id))) {
      navigate('/manage/libraries')
    }
  }

  const changed = name.trim() !== '' && name.trim() !== library.name
  return (
    <section aria-labelledby="library-settings" className={styles.panel}>
      <h2 id="library-settings">Settings</h2>
      <label className={styles.field} htmlFor="library-name">
        Name
      </label>
      <div className={styles.row}>
        <input
          id="library-name"
          className={styles.input}
          style={{ fontFamily: 'inherit' }}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && changed && rename()}
        />
        <button
          type="button"
          className={`${styles.button} ${styles.small}`}
          disabled={!changed || busy}
          onClick={rename}
        >
          Rename
        </button>
      </div>
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        <button
          type="button"
          className={`${styles.button} ${styles.small} ${styles.danger}`}
          disabled={busy || library.roots.length > 0}
          title={library.roots.length > 0 ? 'Remove its folders first' : undefined}
          onClick={remove}
        >
          Delete library
        </button>
        {library.roots.length > 0 && (
          <span className={styles.muted}>Remove its folders first to delete it.</span>
        )}
      </div>
    </section>
  )
}

/** One library: its folders (each leading to its own settings), adding one, and its settings. */
export function LibraryDetail({ id }: { id: number }) {
  const libraries = useLibraries()
  const adding = useQueryParam('add') === '1'
  const setAdding = (on: boolean) =>
    navigate(on ? `/manage/libraries/${id}?add=1` : `/manage/libraries/${id}`, { replace: true })

  if (libraries.error) {
    return <Failed error={libraries.error} onRetry={libraries.reload} />
  }
  if (!libraries.data) {
    return <Loading />
  }
  const library = libraries.data.find((l) => l.id === id)
  if (!library) {
    return <NotFoundPage />
  }
  // A folder library fetches no metadata and never places files.
  const isFolders = library.kind === 'folders'
  // Changes to roots change the placement example; refetch it when they do.
  const version = library.roots
    .map((r) => `${r.id}:${r.write_policy}:${r.layout_template}`)
    .join('|')

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <Link to="/manage/libraries" className={styles.back}>
          <BackIcon />
          Libraries & roots
        </Link>
        <div className={styles.heading}>
          <h1>{library.name}</h1>
          <p className={styles.lead}>{librarySummary(library)}</p>
        </div>

        {!isFolders &&
          library.roots.length > 0 &&
          !library.roots.some((r) => r.is_import_target) && <NoImportTarget library={library} />}

        <section aria-labelledby="library-folders" className={styles.section}>
          <h2 id="library-folders" className={styles.sectionTitle}>
            Folders
          </h2>
          {library.roots.length > 0 ? (
            <ul className={styles.rootList}>
              {library.roots.map((r) => (
                <RootRow key={r.id} root={r} libraryId={library.id} />
              ))}
            </ul>
          ) : (
            <p className={styles.muted}>No folders in this library yet.</p>
          )}
          {adding ? (
            <AddFolder
              library={library}
              onAdded={() => {
                setAdding(false)
                libraries.reload()
              }}
              onCancel={() => setAdding(false)}
            />
          ) : (
            <button
              type="button"
              className={`${styles.button} ${library.roots.length === 0 ? styles.primary : ''}`}
              style={{ alignSelf: 'flex-start' }}
              onClick={() => setAdding(true)}
            >
              Add a folder
            </button>
          )}
        </section>

        {!isFolders && <Metadata library={library} onChanged={libraries.reload} />}
        {library.kind !== 'tv' && <Thumbnails library={library} />}
        {/* Keyed so a rename elsewhere resets the field. */}
        <Settings key={library.name} library={library} onChanged={libraries.reload} />
        {!isFolders && <PlacementPanel libraryId={library.id} version={version} />}
      </div>
    </div>
  )
}
