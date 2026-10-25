import styles from './Page.module.css'

export function NotFoundPage() {
  return (
    <div className={styles.page}>
      <h1 className={styles.title}>Not found</h1>
      <p className={styles.muted}>There's nothing at this address.</p>
    </div>
  )
}
