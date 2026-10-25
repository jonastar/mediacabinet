import { useState } from 'react'
import { api } from '../api/client'
import styles from './EditDetails.module.css'

interface Props {
  itemId: number
  title: string
  year: number | null | undefined
  overview: string | null | undefined
  onSaved: () => void
  onCancel: () => void
}

/** Edit a series' or movie's details; locking keeps them through refreshes. */
export function EditDetails({ itemId, title, year, overview, onSaved, onCancel }: Props) {
  const [draft, setDraft] = useState({
    title,
    year: year ? String(year) : '',
    overview: overview ?? '',
  })
  const [lock, setLock] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const save = async () => {
    setBusy(true)
    setError(null)
    try {
      await api.editItem(itemId, {
        title: draft.title,
        year: draft.year ? Number(draft.year) : null,
        overview: draft.overview,
        lock,
      })
      onSaved()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-labelledby="edit-details" className={styles.panel}>
      <h2 id="edit-details">Edit details</h2>
      <div className={styles.grid}>
        <label className={styles.field}>
          Title
          <input
            className={styles.input}
            value={draft.title}
            onChange={(e) => setDraft({ ...draft, title: e.target.value })}
          />
        </label>
        <label className={styles.field}>
          Year
          <input
            className={styles.input}
            inputMode="numeric"
            value={draft.year}
            onChange={(e) =>
              setDraft({ ...draft, year: e.target.value.replace(/\D/g, '').slice(0, 4) })
            }
          />
        </label>
      </div>
      <label className={styles.field}>
        Overview
        <textarea
          className={styles.input}
          value={draft.overview}
          onChange={(e) => setDraft({ ...draft, overview: e.target.value })}
        />
      </label>
      <label className={styles.check}>
        <input type="checkbox" checked={lock} onChange={(e) => setLock(e.target.checked)} />
        Lock these from metadata refresh
      </label>
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        <button
          type="button"
          className={`${styles.button} ${styles.primary}`}
          disabled={busy || !draft.title.trim()}
          onClick={save}
        >
          Save
        </button>
        <button type="button" className={styles.button} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </section>
  )
}
