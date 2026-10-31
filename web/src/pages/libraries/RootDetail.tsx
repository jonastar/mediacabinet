import { useState } from 'react'
import { api, type Library, type LibraryKind, type Root, type WritePolicy } from '../../api/client'
import { BackIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { Failed, Loading } from '../../components/Status'
import { navigate } from '../../lib/router'
import { NotFoundPage } from '../NotFoundPage'
import { FolderLayout } from './FolderLayout'
import { PolicyPicker } from './PolicyPicker'
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
  const policy = root.write_policy as WritePolicy

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

      <section aria-labelledby="root-policy" className={styles.panel}>
        <h2 id="root-policy">Write policy</h2>
        <PolicyPicker
          name={`policy-${root.id}`}
          value={policy}
          disabled={busy}
          onChange={(value) => act(() => api.updateRoot(root.id, { write_policy: value }))}
        />
        <div className={styles.pathLine}>
          {root.is_import_target ? (
            <span className={styles.tag}>Import target</span>
          ) : policy !== 'read_only' ? (
            <button
              type="button"
              className={`${styles.button} ${styles.small}`}
              disabled={busy}
              onClick={() => act(() => api.updateRoot(root.id, { is_import_target: true }))}
            >
              Make import target
            </button>
          ) : null}
          <span className={styles.muted}>
            {root.is_import_target
              ? `New ${kind === 'movies' ? 'movies' : 'series'} of ${library.name} land in this folder.`
              : `The import target is where new ${kind === 'movies' ? 'movies' : 'series'} of ${library.name} land.`}
          </span>
        </div>
      </section>

      <section aria-labelledby="root-template" className={styles.panel}>
        <h2 id="root-template">Folder layout</h2>
        <FolderLayout
          // Remounted on cancel, so the checkbox follows the saved template again.
          key={resets}
          id={`template-${root.id}`}
          kind={kind}
          policy={policy}
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

/** One folder of a library: scanning, write policy, import target, layout template. */
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
