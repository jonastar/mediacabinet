import styles from './Status.module.css'

/** Loading and error placeholders for a page or section. */
export function Loading({ label = 'Loading…' }: { label?: string }) {
  return (
    <p className={styles.message} role="status">
      {label}
    </p>
  )
}

export function Failed({ error, onRetry }: { error: Error; onRetry?: () => void }) {
  return (
    <div className={styles.message} role="alert">
      <p>Couldn't load this: {error.message}</p>
      {onRetry && (
        <button type="button" className={styles.retry} onClick={onRetry}>
          Try again
        </button>
      )}
    </div>
  )
}
