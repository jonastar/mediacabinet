import styles from './Libraries.module.css'

/** What the tokens of a layout mean, coloured as the editor colours them. */
export function Legend({ kind, part }: { kind: string; part: 'above' | 'below' }) {
  const item = kind === 'movies' ? '{movie}' : '{series}'
  if (part === 'below') {
    return (
      <div className={styles.legend}>
        <span>
          {(kind === 'movies'
            ? ['{edition}', '{resolution}']
            : ['{season:00}', '{episode:00}']
          ).map((token) => (
            <code key={token} className={styles.below}>
              {token}{' '}
            </code>
          ))}
          <code className={styles.below}>{'{title}'}</code> folder and file names
        </span>
        <span>
          <code className={styles.below}>{'{filename}'}</code> keeps the file’s own name;{' '}
          <code className={styles.below}>{'{ext}'}</code> its extension
        </span>
      </div>
    )
  }
  return (
    <div className={styles.legend}>
      <span>
        <code className={styles.grouping}>{'{year}'}</code>{' '}
        <code className={styles.grouping}>{'{genre}'}</code> grouping levels: matching hints, never
        stored as metadata
      </span>
      <span>
        <code className={styles.item}>{item}</code> the {kind === 'movies' ? 'movie' : 'series'}{' '}
        folder, saved once per {kind === 'movies' ? 'movie' : 'series'}
      </span>
    </div>
  )
}
