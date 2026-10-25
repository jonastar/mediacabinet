import { useState } from 'react'
import { api, type LibraryKind, type Root, type WritePolicy } from '../../api/client'
import { POLICIES, POLICY_NOTES, rootStatus } from './roots'
import { TemplateEditor } from './TemplateEditor'
import { TemplateText } from './TemplateText'
import styles from './Libraries.module.css'

interface Props {
  root: Root
  kind: LibraryKind
  onChanged: () => void
}

export function RootCard({ root, kind, onChanged }: Props) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState({ value: root.layout_template, valid: true })
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

  const saveTemplate = async () => {
    if (await act(() => api.updateRoot(root.id, { layout_template: draft.value }))) {
      setEditing(false)
    }
  }

  const remove = () => {
    const ok = window.confirm(
      `Remove ${root.path} from the library? Its files stay on disk; Media Cabinet forgets them.`,
    )
    if (ok) {
      act(() => api.removeRoot(root.id))
    }
  }

  return (
    <li className={`${styles.card} ${root.online ? '' : styles.offline}`}>
      <div className={styles.cardTop}>
        <div className={styles.identity}>
          <div className={styles.pathLine}>
            <span className={styles.path}>{root.path}</span>
            {root.is_import_target && <span className={styles.tag}>Import target</span>}
          </div>
          <span className={`${styles.status} ${styles[status.tone]}`} role="status">
            <span className={styles.dot} aria-hidden="true" />
            {status.text}
          </span>
        </div>
        <div role="group" aria-label={`Write policy for ${root.path}`} className={styles.policies}>
          {POLICIES.map(([value, label]) => (
            <button
              key={value}
              type="button"
              aria-pressed={policy === value}
              disabled={busy}
              onClick={() =>
                policy !== value && act(() => api.updateRoot(root.id, { write_policy: value }))
              }
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className={styles.block}>
        <div className={styles.labelRow}>
          <label className={styles.label} htmlFor={`template-${root.id}`}>
            Layout template
          </label>
          {!editing && (
            <button
              type="button"
              className={styles.link}
              onClick={() => {
                setDraft({ value: root.layout_template, valid: true })
                setEditing(true)
              }}
            >
              Edit
            </button>
          )}
        </div>
        {editing ? (
          <>
            <TemplateEditor
              id={`template-${root.id}`}
              kind={kind}
              value={draft.value}
              onChange={(value, valid) => setDraft({ value, valid })}
            />
            <div className={styles.actions}>
              <button
                type="button"
                className={`${styles.button} ${styles.small} ${styles.primary}`}
                disabled={busy || !draft.valid}
                onClick={saveTemplate}
              >
                Save template
              </button>
              <button
                type="button"
                className={`${styles.button} ${styles.small}`}
                onClick={() => setEditing(false)}
              >
                Cancel
              </button>
            </div>
          </>
        ) : (
          <TemplateText pieces={root.template_pieces} />
        )}
        <p className={styles.note}>{POLICY_NOTES[policy]}</p>
      </div>

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
          onClick={() => act(() => api.scanRoot(root.id))}
        >
          {root.scanning ? 'Scan again after this' : 'Scan now'}
        </button>
        {!root.is_import_target && policy !== 'read_only' && (
          <button
            type="button"
            className={`${styles.button} ${styles.small}`}
            disabled={busy}
            onClick={() => act(() => api.updateRoot(root.id, { is_import_target: true }))}
          >
            Make import target
          </button>
        )}
        <button
          type="button"
          className={`${styles.button} ${styles.small} ${styles.danger}`}
          disabled={busy || root.scanning != null}
          onClick={remove}
        >
          Remove folder
        </button>
      </div>
    </li>
  )
}
