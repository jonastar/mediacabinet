import type { ReactNode } from 'react'
import type { About } from '../../api/client'
import { CoverImage } from '../../components/Artwork'
import { EditModeToggle } from '../../components/EditModeToggle'
import { BackIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { Ratings } from '../../components/People'
import type { Fact } from './metadata'
import styles from './Detail.module.css'

/** The way back from a page, one link per level: `Series / The Show / Season 2`. */
export function Breadcrumbs({ crumbs }: { crumbs: { to: string; label: string }[] }) {
  return (
    <nav aria-label="Breadcrumb" className={styles.crumbs}>
      <BackIcon />
      {crumbs.map((c, i) => (
        <span key={c.to} className={styles.crumb}>
          {i > 0 && <span aria-hidden="true">/</span>}
          <Link to={c.to}>{c.label}</Link>
        </span>
      ))}
    </nav>
  )
}

function FactsCard({ items }: { items: Fact[] }) {
  if (items.length === 0) {
    return null
  }
  return (
    <dl className={styles.facts}>
      {items.map((f) => (
        <div key={f.label} className={styles.fact}>
          <dt>{f.label}</dt>
          <dd>{f.value}</dd>
        </div>
      ))}
    </dl>
  )
}

/** The poster or still at the top of a page's left column. */
export function DetailArtwork({
  src,
  tone,
  wide = false,
  editing = false,
  label,
  children,
}: {
  src: string | null
  tone: string
  /** A 16:9 still rather than a 2:3 poster. */
  wide?: boolean
  editing?: boolean
  /** Shown when there's no picture. */
  label?: string
  children?: ReactNode
}) {
  return (
    <div
      className={`${styles.art} ${wide ? styles.artWide : ''} ${editing ? styles.artEditing : ''}`}
      style={{ background: tone }}
    >
      {label && <span className={styles.artLabel}>{label}</span>}
      <CoverImage src={src} />
      {children}
    </div>
  )
}

/** `2000 · Animation · Adult Swim · Ended`, followed by the age rating and scores. */
export function MetaLine({
  parts,
  about,
}: {
  parts: (string | number | null | undefined)[]
  about?: About | null
}) {
  const shown = parts.filter((p) => p != null && p !== '')
  return (
    <div className={styles.meta}>
      {shown.map((p, i) => (
        <span key={p} className={styles.metaPart}>
          {i > 0 && <span aria-hidden="true">·</span>}
          <span>{p}</span>
        </span>
      ))}
      <Ratings about={about} />
    </div>
  )
}

/** The numbers an editor looks at first. */
export function HealthTiles({
  title,
  tiles,
}: {
  title: string
  tiles: { label: string; value: ReactNode; warn?: boolean }[]
}) {
  return (
    <section className={styles.featured} aria-label={title}>
      <h2 className={styles.featuredTitle}>{title}</h2>
      <div className={styles.tiles}>
        {tiles.map((t) => (
          <div key={t.label} className={styles.tile}>
            <span className={styles.tileLabel}>{t.label}</span>
            <span className={`${styles.tileValue} ${t.warn ? styles.tileWarn : ''}`}>
              {t.value}
            </span>
          </div>
        ))}
      </div>
    </section>
  )
}

/**
 * The layout every detail page shares: a backdrop band, a top bar, artwork and
 * facts on the left, and the page's text, actions and featured block on the right.
 */
export function DetailHeader({
  titleId,
  backdrop,
  tone,
  back,
  barActions,
  artwork,
  wide = false,
  facts: items,
  children,
  below,
}: {
  titleId: string
  backdrop: string | null
  tone: string
  back: ReactNode
  /** Edit-mode buttons beside the switch. */
  barActions?: ReactNode
  artwork: ReactNode
  wide?: boolean
  facts: Fact[]
  children: ReactNode
  /** The sections under the header. */
  below?: ReactNode
}) {
  return (
    <div className={styles.page}>
      <div className={styles.backdrop} style={{ background: tone }} aria-hidden="true">
        <CoverImage src={backdrop} />
        <span className={styles.scrim} />
      </div>
      <div className={styles.container}>
        <div className={styles.topBar}>
          {back}
          <div className={styles.barActions}>
            {barActions}
            <EditModeToggle />
          </div>
        </div>
        <section aria-labelledby={titleId} className={styles.header}>
          <div className={`${styles.aside} ${wide ? styles.asideWide : ''}`}>
            {artwork}
            <FactsCard items={items} />
          </div>
          <div className={styles.body}>{children}</div>
        </section>
        {below}
      </div>
    </div>
  )
}
