import styles from './Page.module.css'

/** Screens on the roadmap that aren't built yet. */
export function PlaceholderPage({ title, phase }: { title: string; phase: string }) {
  return (
    <div className={styles.page}>
      <h1 className={styles.title}>{title}</h1>
      <p className={styles.muted}>Not built yet: planned for {phase} (see docs/roadmap.md).</p>
    </div>
  )
}

export function NotFoundPage() {
  return (
    <div className={styles.page}>
      <h1 className={styles.title}>Not found</h1>
      <p className={styles.muted}>There's nothing at this address.</p>
    </div>
  )
}
