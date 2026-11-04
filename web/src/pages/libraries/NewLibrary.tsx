import { useState } from 'react'
import { api, type Library, type LibraryKind } from '../../api/client'
import { Segmented } from '../../components/Segmented'
import styles from './Libraries.module.css'

const PLACEHOLDERS: Record<LibraryKind, string> = {
  tv: 'TV Shows',
  movies: 'Movies',
  folders: 'Home Videos',
}

export function NewLibrary({
  onCreated,
  onCancel,
}: {
  onCreated: (l: Library) => void
  onCancel?: () => void
}) {
  const [name, setName] = useState('')
  const [kind, setKind] = useState<LibraryKind>('tv')
  const [error, setError] = useState<string | null>(null)

  const create = async () => {
    setError(null)
    try {
      onCreated(await api.createLibrary(name, kind))
    } catch (e) {
      setError((e as Error).message)
    }
  }

  return (
    <section aria-labelledby="new-library" className={styles.panel}>
      <h2 id="new-library">New library</h2>
      <p className={styles.muted}>
        A library holds one kind of media and can span several folders.
      </p>
      <label className={styles.field}>
        Name
        <input
          className={styles.input}
          style={{ fontFamily: 'inherit' }}
          value={name}
          placeholder={PLACEHOLDERS[kind]}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && name && create()}
        />
      </label>
      <Segmented
        label="Kind"
        variant="pills"
        value={kind}
        onChange={setKind}
        options={[
          { value: 'tv', label: 'TV shows' },
          { value: 'movies', label: 'Movies' },
          { value: 'folders', label: 'Folders' },
        ]}
      />
      {kind === 'folders' && (
        <p className={styles.muted}>
          For home videos, clips and anything else no database knows about. You browse your folders
          as they are, with each video named after its file. No metadata is fetched, and nothing in
          the folders is ever changed.
        </p>
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
          disabled={!name.trim()}
          onClick={create}
        >
          Create library
        </button>
        {onCancel && (
          <button type="button" className={styles.button} onClick={onCancel}>
            Cancel
          </button>
        )}
      </div>
    </section>
  )
}
