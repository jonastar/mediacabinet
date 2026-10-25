import { useState } from 'react'
import {
  api,
  type FolderListing,
  type FolderProposal,
  type Library,
  type LibraryKind,
  type WritePolicy,
} from '../../api/client'
import { POLICIES, POLICY_NOTES } from './roots'
import { TemplateEditor } from './TemplateEditor'
import styles from './Libraries.module.css'

interface Props {
  library: Library
  onAdded: () => void
  onCancel: () => void
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
          <p className={styles.muted}>
            {proposal.files === 0
              ? 'No videos here yet.'
              : `${proposal.grouped.toLocaleString()} of ${proposal.files.toLocaleString()} videos fit this layout`}
            {proposal.examples.length > 0 && `, e.g. ${proposal.examples.slice(0, 3).join(', ')}`}.{' '}
            {proposal.reason}
          </p>
          <div className={styles.block}>
            <label className={styles.label} htmlFor="add-template">
              Layout template
            </label>
            <TemplateEditor
              id="add-template"
              kind={kind}
              value={template.value}
              onChange={(value, valid) => setTemplate({ value, valid })}
            />
          </div>
          <div className={styles.block}>
            <span className={styles.label}>Write policy</span>
            <div
              role="group"
              aria-label="Write policy"
              className={styles.policies}
              style={{ alignSelf: 'flex-start' }}
            >
              {POLICIES.map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  aria-pressed={policy === value}
                  onClick={() => setPolicy(value)}
                >
                  {label}
                </button>
              ))}
            </div>
            <p className={styles.note}>{POLICY_NOTES[policy]}</p>
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
      </div>
    </section>
  )
}
