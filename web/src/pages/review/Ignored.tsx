import { useState } from 'react'
import { api, type IgnoredFolder } from '../../api/client'
import styles from './Review.module.css'

/** Folders ignored from the queue, each with a way back. */
export function Ignored({ folders, onDone }: { folders: IgnoredFolder[]; onDone: () => void }) {
  const [busy, setBusy] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)

  const restore = async (id: number) => {
    setBusy(id)
    setError(null)
    try {
      await api.unignoreFolder(id)
      onDone()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(null)
    }
  }

  return (
    <section aria-labelledby="review-ignored" className={styles.ignored}>
      <h2 id="review-ignored" className={styles.label}>
        Ignored folders
      </h2>
      <p className={styles.muted}>
        Their files, and any that appear in them later, are left out of matching. Stop ignoring one
        to match it again.
      </p>
      <ul className={styles.ignoredList}>
        {folders.map((f) => (
          <li key={f.id} className={styles.ignoredRow}>
            <span className={styles.candidateText}>
              <span className={styles.entryName}>{f.folder_path}</span>
              <span className={styles.entryMeta}>
                {f.files} {f.files === 1 ? 'file' : 'files'} · {f.root_path}
              </span>
            </span>
            <button
              type="button"
              className={styles.button}
              disabled={busy !== null}
              onClick={() => restore(f.id)}
            >
              {busy === f.id ? 'Matching…' : 'Stop ignoring'}
            </button>
          </li>
        ))}
      </ul>
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
    </section>
  )
}
