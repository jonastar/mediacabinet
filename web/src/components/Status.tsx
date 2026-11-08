import markUrl from '../assets/mark.svg'
import styles from './Status.module.css'

/**
 * Loading and error placeholders for a page or section. Loading fills the space
 * it's in with the still mark, faded in only once a load takes a moment so quick
 * page switches don't flash it.
 */
export function Loading({ label }: { label?: string }) {
  return (
    <div className={styles.loading} role="status">
      <img src={markUrl} alt="Loading" width={44} height={50} />
      {label && <span className={styles.label}>{label}</span>}
    </div>
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
