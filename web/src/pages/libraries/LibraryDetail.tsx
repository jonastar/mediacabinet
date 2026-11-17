import { useState } from 'react'
import { api, type Library } from '../../api/client'
import { BackIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { Failed, Loading } from '../../components/Status'
import { navigate, useQueryParam } from '../../lib/router'
import { NotFoundPage } from '../NotFoundPage'
import { useApi } from '../../api/useApi'
import { AddFolder } from './AddFolder'
import { FolderBrowser } from './FolderBrowser'
import { PlacementPanel } from './PlacementPanel'
import { librarySummary, refreshStatus } from './roots'
import { RootRow } from './RootRow'
import { useLibraries } from './useLibraries'
import styles from './Libraries.module.css'

/** Which writable folder new series or movies go to: imports land there. */
function NewItems({ library, onChanged }: { library: Library; onChanged: () => void }) {
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const things = library.kind === 'movies' ? 'movies' : 'series'
  const writable = library.roots.filter((r) => !r.read_only)
  const current = library.roots.find((r) => r.is_import_target)

  const choose = async (rootId: number) => {
    setBusy(true)
    setError(null)
    try {
      await api.updateRoot(rootId, { is_import_target: true })
      onChanged()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <section
      aria-labelledby="library-new-items"
      className={`${styles.panel} ${current ? '' : styles.warning}`}
    >
      <h2 id="library-new-items">New items</h2>
      {writable.length === 0 ? (
        <p className={styles.muted} role="alert">
          Every folder here is read-only, so {library.name} can’t import new {things}. Make a folder
          writable, or add one Media Cabinet may write to.
        </p>
      ) : (
        <>
          <label className={styles.field} htmlFor="library-new-items-root">
            New {things} go to
          </label>
          <select
            id="library-new-items-root"
            className={styles.input}
            value={current?.id ?? ''}
            disabled={busy}
            onChange={(e) => e.target.value && choose(Number(e.target.value))}
          >
            {!current && <option value="">Choose a folder…</option>}
            {writable.map((r) => (
              <option key={r.id} value={r.id}>
                {r.path}
              </option>
            ))}
          </select>
          <p className={styles.muted} role={current ? undefined : 'alert'}>
            {current
              ? `Imports of ${things} already here go to their own folder, unless it’s read-only.`
              : `Choose a folder, or ${library.name} can’t import new ${things}.`}
          </p>
        </>
      )}
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
    </section>
  )
}

/** A folder whose finished videos are imported on their own, and what in it needs a person. */
function InboxPanel({ library, onChanged }: { library: Library; onChanged: () => void }) {
  const [path, setPath] = useState(library.inbox_path ?? '')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [checking, setChecking] = useState(false)
  const inbox = useApi(
    library.inbox_path ? `inbox:${library.id}:${library.inbox_path}` : null,
    () => api.inbox(library.id),
  )

  const save = async (next: string) => {
    setBusy(true)
    setError(null)
    try {
      await api.setInbox(library.id, next)
      onChanged()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const check = async () => {
    setError(null)
    try {
      await api.checkInbox(library.id)
      setChecking(true)
    } catch (e) {
      setError((e as Error).message)
    }
  }

  const held = inbox.data?.attention ?? []
  return (
    <section aria-labelledby="library-inbox" className={styles.panel}>
      <h2 id="library-inbox">Inbox</h2>
      <p className={styles.muted}>
        A folder Media Cabinet owns: finished videos that land in it are imported into{' '}
        {library.name} by the layout. Point a download client’s “move completed downloads to” folder
        here. Files are hard linked when they’re on the same disk as the library, so the client
        keeps seeding, and moved otherwise. Anything that can’t be placed stays in the inbox for you
        to look at.
      </p>
      <FolderBrowser
        id="inbox-path"
        label="Inbox folder"
        value={path}
        placeholder="/downloads/complete/tv"
        action={busy ? 'Saving…' : library.inbox_path ? 'Change' : 'Use this folder'}
        busy={busy || path.trim() === (library.inbox_path ?? '')}
        onChange={setPath}
        onSubmit={save}
      />
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      {library.inbox_path && (
        <>
          <span
            className={`${styles.status} ${styles[inbox.data?.online === false ? 'bad' : 'ok']}`}
            role="status"
          >
            <span className={styles.dot} aria-hidden="true" />
            {inbox.data?.online === false
              ? 'The folder can’t be reached'
              : `Looked at every minute${held.length ? ` · ${held.length} need${held.length === 1 ? 's' : ''} you` : ''}`}
          </span>
          {held.length > 0 && (
            <ul className={styles.inboxList}>
              {held.map((f) => (
                <li key={f.path}>
                  <span className={styles.mono}>{f.relative_path}</span>
                  <span className={styles.muted}>{f.reason}</span>
                </li>
              ))}
            </ul>
          )}
          <div className={styles.actions}>
            {held.length > 0 && (
              <Link
                to={`/manage/libraries/${library.id}/import?path=${encodeURIComponent(library.inbox_path)}`}
                className={`${styles.button} ${styles.small} ${styles.buttonLink}`}
              >
                Import by hand
              </Link>
            )}
            <button type="button" className={`${styles.button} ${styles.small}`} onClick={check}>
              Look again now
            </button>
            <button
              type="button"
              className={`${styles.button} ${styles.small} ${styles.danger}`}
              disabled={busy}
              onClick={() => {
                setPath('')
                save('')
              }}
            >
              Stop using it
            </button>
            {checking && <Link to="/manage/tasks">See how it’s going in Tasks</Link>}
          </div>
        </>
      )}
    </section>
  )
}

/** Bringing files in from a folder outside the library. */
function Import({ library }: { library: Library }) {
  const things = library.kind === 'movies' ? 'movies' : 'episodes'
  return (
    <section aria-labelledby="library-import" className={styles.panel}>
      <h2 id="library-import">Import</h2>
      <p className={styles.muted}>
        Bring {things} in from a folder on the server, such as finished downloads. You check what
        each file is and see where it goes first. Copying is the default, so the originals stay put.
      </p>
      <div className={styles.actions}>
        <Link
          to={`/manage/libraries/${library.id}/import`}
          className={`${styles.button} ${styles.small} ${styles.buttonLink}`}
        >
          Import files
        </Link>
      </div>
    </section>
  )
}

/** Renaming files to their folder's layout, shown first and chosen file by file. */
function Organise({ library }: { library: Library }) {
  const writable = library.roots.some((r) => !r.read_only)
  return (
    <section aria-labelledby="library-organise" className={styles.panel}>
      <h2 id="library-organise">Organise</h2>
      <p className={styles.muted}>
        Rename and move files to match each folder’s layout. You see every change first and choose
        which to make. Nothing is ever organised on its own, and read-only folders are left alone.
      </p>
      <div className={styles.actions}>
        {writable ? (
          <Link
            to={`/manage/libraries/${library.id}/organize`}
            className={`${styles.button} ${styles.small} ${styles.buttonLink}`}
          >
            Preview changes
          </Link>
        ) : (
          <span className={styles.muted}>Every folder here is read-only.</span>
        )}
      </div>
    </section>
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
  const version = library.roots.map((r) => `${r.id}:${r.read_only}:${r.layout_template}`).join('|')

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

        {!isFolders && library.roots.length > 0 && (
          <NewItems library={library} onChanged={libraries.reload} />
        )}
        {!isFolders && library.roots.some((r) => !r.read_only) && <Import library={library} />}
        {!isFolders && library.roots.some((r) => !r.read_only) && (
          <InboxPanel
            key={library.inbox_path ?? ''}
            library={library}
            onChanged={libraries.reload}
          />
        )}
        {!isFolders && library.roots.length > 0 && <Organise library={library} />}
        {!isFolders && <Metadata library={library} onChanged={libraries.reload} />}
        {library.kind !== 'tv' && <Thumbnails library={library} />}
        {/* Keyed so a rename elsewhere resets the field. */}
        <Settings key={library.name} library={library} onChanged={libraries.reload} />
        {!isFolders && <PlacementPanel libraryId={library.id} version={version} />}
      </div>
    </div>
  )
}
