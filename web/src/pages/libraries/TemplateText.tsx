import type { TemplatePiece } from '../../api/client'
import styles from './Libraries.module.css'

/** A layout template coloured by level: grouping, the item folder, below it. */
export function TemplateText({ pieces }: { pieces: TemplatePiece[] }) {
  return (
    <p className={styles.template}>
      {pieces.map((p, i) => (
        <span key={i} className={styles[p.role]}>
          {p.text}
        </span>
      ))}
    </p>
  )
}

export function Legend({ kind }: { kind: string }) {
  const item = kind === 'movies' ? '{movie}' : '{series}'
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
      <span>
        <code className={styles.below}>{'{season:00}'}</code> optional levels below it
      </span>
    </div>
  )
}
