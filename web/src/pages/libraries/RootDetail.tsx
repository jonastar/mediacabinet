import { useState } from 'react'
import { api, type Library, type LibraryKind, type Root } from '../../api/client'
import { BackIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { Failed, Loading } from '../../components/Status'
import { navigate } from '../../lib/router'
import { NotFoundPage } from '../NotFoundPage'
import { AccessPicker } from './AccessPicker'
import { FolderLayout } from './FolderLayout'
import { rootStatus } from './roots'
import { useLibraries } from './useLibraries'
import styles from './Libraries.module.css'

function Folder({
  library,
  root,
  onChanged,
}: {
  library: Library
  root: Root
  onChanged: () => void
}) {
  const kind = library.kind as LibraryKind
  const [draft, setDraft] = useState({ value: root.layout_template, valid: true })
  const [resets, setResets] = useState(0)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const status = rootStatus(root)

  const act = async (action: () => Promise<unknown>) => {
    setBusy(true)
    setError(null)
    try {
      await action()
      onChanged()
      return true
    } catch (e) {
      setError((e as Error).message)
      return false
    } finally {
      setBusy(false)
    }
  }

  const saveTemplate = () => act(() => api.updateRoot(root.id, { layout_template: draft.value }))
  const templateChanged = draft.value !== root.layout_template

  const remove = async () => {
    const ok = window.confirm(
      `Remove ${root.path} from the library? Its files stay on disk; Media Cabinet forgets them.`,
    )
    if (ok && (await act(() => api.removeRoot(root.id)))) {
      navigate(`/manage/libraries/${library.id}`)
    }
  }

  return (
    <>
      <div className={styles.heading}>
        <h1 className={styles.rootTitle}>{root.path}</h1>
        <span className={`${styles.status} ${styles[status.tone]}`} role="status">
          <span className={styles.dot} aria-hidden="true" />
          {status.text}
        </span>
      </div>

      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}

      <section aria-labelledby="root-scan" className={styles.panel}>
        <h2 id="root-scan">Scanning</h2>
        <p className={styles.muted}>
          Folders are watched for changes and rescanned on a schedule. Scan now to pick up changes
          straight away.
        </p>
        <div className={styles.actions}>
          <button
            type="button"
            className={`${styles.button} ${styles.small}`}
            disabled={busy}
            onClick={() => act(() => api.scanRoot(root.id))}
          >
            {root.scanning ? 'Scan again after this' : 'Scan now'}
          </button>
        </div>
      </section>

      {kind !== 'folders' && (
        <>
          <section aria-labelledby="root-access" className={styles.panel}>
            <h2 id="root-access">Writing</h2>
            <AccessPicker
              name={`access-${root.id}`}
              readOnly={root.read_only}
              disabled={busy || (root.is_import_target && !root.read_only)}
              onChange={(readOnly) => act(() => api.updateRoot(root.id, { read_only: readOnly }))}
            />
            {root.is_import_target && (
              <p className={styles.muted}>
                New {kind === 'movies' ? 'movies' : 'series'} of {library.name} go to this folder.
                To make it read-only, choose another folder for new items on{' '}
                <Link to={`/manage/libraries/${library.id}`}>the library’s page</Link> first.
              </p>
            )}
          </section>

          {!root.read_only && (
            <section aria-labelledby="root-organise" className={styles.panel}>
              <h2 id="root-organise">Organise</h2>
              <p className={styles.muted}>
                Rename and move this folder’s files to match its layout, after seeing every change.
              </p>
              <div className={styles.actions}>
                <Link
                  to={`/manage/libraries/${library.id}/organize?root=${root.id}`}
                  className={`${styles.button} ${styles.small} ${styles.buttonLink}`}
                >
                  Preview changes
                </Link>
              </div>
            </section>
          )}

          <section aria-labelledby="root-template" className={styles.panel}>
            <h2 id="root-template">Folder layout</h2>
            <FolderLayout
              // Remounted on cancel, so the checkbox follows the saved template again.
              key={resets}
              id={`template-${root.id}`}
              kind={kind}
              readOnly={root.read_only}
              value={draft.value}
              onChange={(value, valid) => setDraft({ value, valid })}
            />
            {templateChanged && (
              <div className={styles.actions}>
                <button
                  type="button"
                  className={`${styles.button} ${styles.small} ${styles.primary}`}
                  disabled={busy || !draft.valid}
                  onClick={saveTemplate}
                >
                  Save layout
                </button>
                <button
                  type="button"
                  className={`${styles.button} ${styles.small}`}
                  onClick={() => {
                    setDraft({ value: root.layout_template, valid: true })
                    setResets((n) => n + 1)
                  }}
                >
                  Cancel
                </button>
              </div>
            )}
          </section>
        </>
      )}

      <section aria-labelledby="root-remove" className={styles.panel}>
        <h2 id="root-remove">Remove</h2>
        <p className={styles.muted}>
          Its files stay on disk; Media Cabinet forgets them and what they were matched to.
        </p>
        <div className={styles.actions}>
          <button
            type="button"
            className={`${styles.button} ${styles.small} ${styles.danger}`}
            disabled={busy || root.scanning != null}
            title={root.scanning ? 'Wait for the scan to finish' : undefined}
            onClick={remove}
          >
            Remove folder
          </button>
        </div>
      </section>
    </>
  )
}

/** One folder of a library: scanning, writable or read-only, layout template. */
export function RootDetail({ libraryId, id }: { libraryId: number; id: number }) {
  const libraries = useLibraries()
  if (libraries.error) {
    return <Failed error={libraries.error} onRetry={libraries.reload} />
  }
  if (!libraries.data) {
    return <Loading />
  }
  const library = libraries.data.find((l) => l.id === libraryId)
  const root = library?.roots.find((r) => r.id === id)
  if (!library || !root) {
    return <NotFoundPage />
  }
  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <Link to={`/manage/libraries/${library.id}`} className={styles.back}>
          <BackIcon />
          {library.name}
        </Link>
        {/* Keyed so a saved template becomes the new starting point. */}
        <Folder
          key={root.layout_template}
          library={library}
          root={root}
          onChanged={libraries.reload}
        />
      </div>
    </div>
  )
}
