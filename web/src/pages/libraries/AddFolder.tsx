import { useState } from 'react'
import {
  api,
  type FolderListing,
  type FolderProposal,
  type Library,
  type LibraryKind,
  type WritePolicy,
} from '../../api/client'
import { FolderLayout } from './FolderLayout'
import { PolicyPicker } from './PolicyPicker'
import styles from './Libraries.module.css'

interface Props {
  library: Library
  onAdded: () => void
  onCancel: () => void
}

/** How well the folder fits the proposed layout: green when every video does. */
function Fit({ proposal }: { proposal: FolderProposal }) {
  const examples =
    proposal.examples.length > 0 ? `, e.g. ${proposal.examples.slice(0, 3).join(', ')}` : ''
  if (proposal.files === 0) {
    return <p className={styles.muted}>No videos here yet. {proposal.reason}</p>
  }
  const all = proposal.grouped === proposal.files
  const tone = all ? styles.fitOk : proposal.grouped === 0 ? styles.fitBad : styles.fitPartial
  return (
    <div className={`${styles.fit} ${tone}`} role="status">
      <strong>
        {all
          ? `All ${proposal.files.toLocaleString()} videos fit this layout. Nothing to change.`
          : `${proposal.grouped.toLocaleString()} of ${proposal.files.toLocaleString()} videos fit this layout.`}
      </strong>
      <span>
        {examples && `Found${examples.slice(1)}. `}
        {proposal.reason}
      </span>
    </div>
  )
}

/** Pick a folder, see the proposed template, confirm it with a write policy. */
export function AddFolder({ library, onAdded, onCancel }: Props) {
  const kind = library.kind as LibraryKind
  const [path, setPath] = useState('')
  const [browser, setBrowser] = useState<FolderListing | null>(null)
  const [proposal, setProposal] = useState<FolderProposal | null>(null)
  const [template, setTemplate] = useState({ value: '', valid: false })
  const [policy, setPolicy] = useState<WritePolicy>('adaptive')
  const [importTarget, setImportTarget] = useState(!library.roots.some((r) => r.is_import_target))
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const browse = async (at: string) => {
    setError(null)
    try {
      setBrowser(await api.folders(at || '/'))
    } catch (e) {
      setError((e as Error).message)
    }
  }

  const check = async (at = path) => {
    setBusy(true)
    setError(null)
    setProposal(null)
    try {
      const p = await api.propose(at, kind)
      setProposal(p)
      setPath(p.path)
      setTemplate({ value: p.template, valid: true })
      setBrowser(null)
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const add = async () => {
    if (!proposal) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      await api.addRoot(library.id, {
        path: proposal.path,
        write_policy: policy,
        layout_template: template.value,
        is_import_target: importTarget && policy !== 'read_only',
      })
      onAdded()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-labelledby="add-folder" className={styles.panel}>
      <h2 id="add-folder">Add a folder to {library.name}</h2>
      <label className={styles.field} htmlFor="add-path">
        Folder on the server
      </label>
      <div className={styles.row}>
        <input
          id="add-path"
          className={styles.input}
          value={path}
          placeholder="/media/tv"
          spellCheck={false}
          onChange={(e) => {
            setPath(e.target.value)
            setProposal(null)
          }}
          onKeyDown={(e) => e.key === 'Enter' && path && check()}
        />
        <button
          type="button"
          className={`${styles.button} ${styles.small}`}
          onClick={() => browse(path)}
        >
          Browse…
        </button>
        <button
          type="button"
          className={`${styles.button} ${styles.small} ${proposal ? '' : styles.primary}`}
          disabled={!path || busy}
          onClick={() => check()}
        >
          {busy && !proposal ? 'Looking…' : 'Look at folder'}
        </button>
      </div>

      {browser && (
        <div className={styles.browser} role="listbox" aria-label={`Folders in ${browser.path}`}>
          {browser.parent && (
            <button type="button" onClick={() => browse(browser.parent ?? '/')}>
              ..
            </button>
          )}
          {browser.folders.map((name) => {
            const full = `${browser.path.replace(/\/$/, '')}/${name}`
            return (
              <button
                key={name}
                type="button"
                onClick={() => {
                  setPath(full)
                  browse(full)
                }}
              >
                {name}/
              </button>
            )
          })}
          {browser.folders.length === 0 && (
            <p className={styles.muted} style={{ padding: 12 }}>
              No subfolders.
            </p>
          )}
        </div>
      )}

      {proposal && (
        <>
          <Fit proposal={proposal} />
          <div className={styles.block}>
            <span className={styles.label}>Folder layout</span>
            <FolderLayout
              // Remounted for each folder looked at, so the checkbox follows its proposal.
              key={proposal.path}
              id="add-template"
              kind={kind}
              policy={policy}
              value={template.value}
              onChange={(value, valid) => setTemplate({ value, valid })}
            />
          </div>
          <div className={styles.block}>
            <span className={styles.label}>Write policy</span>
            <PolicyPicker name="add-policy" value={policy} onChange={setPolicy} />
          </div>
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={importTarget && policy !== 'read_only'}
              disabled={policy === 'read_only'}
              onChange={(e) => setImportTarget(e.target.checked)}
            />
            Import target: new series land here
          </label>
        </>
      )}

      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        <button
          type="button"
          className={`${styles.button} ${styles.primary}`}
          disabled={!proposal || !template.valid || busy}
          onClick={add}
        >
          Add and scan
        </button>
        <button type="button" className={styles.button} onClick={onCancel}>
          Cancel
        </button>
        {!proposal && !busy && (
          <span className={styles.muted}>
            Pick a folder, then Look at folder to see how it’s laid out.
          </span>
        )}
      </div>
    </section>
  )
}
