import { useState } from 'react'
import { api, type FolderListing } from '../../api/client'
import styles from './Libraries.module.css'

interface Props {
  id: string
  label: string
  value: string
  placeholder?: string
  /** What the main button says, e.g. "Look at folder". */
  action: string
  busy?: boolean
  onChange: (path: string) => void
  onSubmit: (path: string) => void
}

/** A server folder: typed in, or picked by browsing the server's folders. */
export function FolderBrowser({
  id,
  label,
  value,
  placeholder,
  action,
  busy,
  onChange,
  onSubmit,
}: Props) {
  const [listing, setListing] = useState<FolderListing | null>(null)
  const [error, setError] = useState<string | null>(null)

  const browse = async (at: string) => {
    setError(null)
    try {
      setListing(await api.folders(at || '/'))
    } catch (e) {
      setError((e as Error).message)
    }
  }

  return (
    <>
      <label className={styles.field} htmlFor={id}>
        {label}
      </label>
      <div className={styles.row}>
        <input
          id={id}
          className={styles.input}
          value={value}
          placeholder={placeholder}
          spellCheck={false}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && value) {
              setListing(null)
              onSubmit(value)
            }
          }}
        />
        <button
          type="button"
          className={`${styles.button} ${styles.small}`}
          onClick={() => browse(value)}
        >
          Browse…
        </button>
        <button
          type="button"
          className={`${styles.button} ${styles.small} ${styles.primary}`}
          disabled={!value || busy}
          onClick={() => {
            setListing(null)
            onSubmit(value)
          }}
        >
          {action}
        </button>
      </div>
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      {listing && (
        <div className={styles.browser} role="listbox" aria-label={`Folders in ${listing.path}`}>
          {listing.parent && (
            <button type="button" onClick={() => browse(listing.parent ?? '/')}>
              ..
            </button>
          )}
          {listing.folders.map((name) => {
            const full = `${listing.path.replace(/\/$/, '')}/${name}`
            return (
              <button
                key={name}
                type="button"
                onClick={() => {
                  onChange(full)
                  browse(full)
                }}
              >
                {name}/
              </button>
            )
          })}
          {listing.folders.length === 0 && (
            <p className={styles.muted} style={{ padding: 12 }}>
              No subfolders.
            </p>
          )}
        </div>
      )}
    </>
  )
}
