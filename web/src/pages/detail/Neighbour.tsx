import { artworkUrl, type Episode } from '../../api/client'
import { CoverImage } from '../../components/Artwork'
import { ArrowLeftIcon, ArrowRightIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { episodeLabel, episodeTone } from '../../lib/format'
import styles from './Detail.module.css'

/** The episode before or after one, with an arrow on the side it's on. */
export function Neighbour({
  episode: e,
  to,
  next = false,
  highlight = false,
}: {
  episode: Episode
  to: string
  next?: boolean
  /** Outlined in the accent: the one to watch. */
  highlight?: boolean
}) {
  const still = artworkUrl({ id: e.item_id, artwork: e.artwork }, 'still')
  const className = [
    styles.neighbour,
    next ? styles.neighbourNext : '',
    highlight ? styles.neighbourOn : '',
  ].join(' ')
  return (
    <Link to={to} className={className}>
      {!next && <ArrowLeftIcon />}
      <span className={styles.neighbourStill} style={{ background: episodeTone(e.item_id) }}>
        <CoverImage src={still} />
      </span>
      <span className={styles.neighbourText}>
        <span className={styles.neighbourLabel}>
          {next ? 'Next' : 'Previous'} · {episodeLabel(e.number)}
        </span>
        <span className={styles.neighbourTitle}>{e.title}</span>
      </span>
      {next && <ArrowRightIcon />}
    </Link>
  )
}

export function NoNeighbour({ next = false }: { next?: boolean }) {
  return (
    <div className={styles.noNeighbour}>
      <span className={styles.neighbourLabel}>{next ? 'Next' : 'Previous'}</span>
      <span>{next ? 'Last episode' : 'First episode'}</span>
    </div>
  )
}
