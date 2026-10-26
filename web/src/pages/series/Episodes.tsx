import type { ReactNode } from 'react'
import type { Episode } from '../../api/client'
import { Badge } from '../../components/Badge'
import { MoreIcon, PlayIcon, WatchedIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { Progress } from '../../components/Progress'
import type { Layout, Mode } from '../../lib/prefs'
import { episodeView, type EpisodeView } from './episodeView'
import styles from './Episodes.module.css'

interface Props {
  entries: Episode[]
  upNext: number | null | undefined
  mode: Mode
  layout: Layout
  /** Episodes whose monitor change is in flight, with the value being saved. */
  pendingMonitor: Map<number, boolean>
  onToggleMonitor: (episode: Episode) => void
  /** Marks episodes watched or not (watch mode). */
  onSetPlayed: (episodes: Episode[], played: boolean) => void
}

export function Episodes({
  entries,
  upNext,
  mode,
  layout,
  pendingMonitor,
  onToggleMonitor,
  onSetPlayed,
}: Props) {
  const views = entries.map((e, i) => episodeView(e, i, upNext, mode))
  const monitored = (v: EpisodeView) => pendingMonitor.get(v.episode.item_id) ?? v.episode.monitored
  return layout === 'list' ? (
    <EpisodeList
      views={views}
      mode={mode}
      monitored={monitored}
      onToggle={onToggleMonitor}
      onSetPlayed={onSetPlayed}
    />
  ) : (
    <EpisodeGrid
      views={views}
      mode={mode}
      monitored={monitored}
      onToggle={onToggleMonitor}
      onSetPlayed={onSetPlayed}
    />
  )
}

interface ViewProps {
  views: EpisodeView[]
  mode: Mode
  monitored: (v: EpisodeView) => boolean
  onToggle: (episode: Episode) => void
  onSetPlayed: (episodes: Episode[], played: boolean) => void
}

const played = (e: Episode) => e.watch?.played ?? false
const playable = (e: Episode) => e.availability === 'available'

/** The thumbnail: plays the episode when there's a file. */
function Thumb({
  v,
  className,
  children,
}: {
  v: EpisodeView
  className: string
  children: ReactNode
}) {
  const style = { background: v.tone }
  if (!playable(v.episode)) {
    return (
      <div className={className} style={style}>
        {children}
      </div>
    )
  }
  return (
    <Link
      to={`/play/${v.episode.item_id}`}
      className={`${className} ${styles.playable}`}
      style={style}
      aria-label={`Play ${v.episode.title}`}
    >
      {children}
      <span className={styles.playOverlay} aria-hidden="true">
        <PlayIcon />
      </span>
    </Link>
  )
}

function WatchedToggle({
  episode,
  onSetPlayed,
}: {
  episode: Episode
  onSetPlayed: ViewProps['onSetPlayed']
}) {
  const watched = played(episode)
  return (
    <button
      type="button"
      className={`${styles.watched} ${watched ? styles.isWatched : ''}`}
      aria-pressed={watched}
      aria-label={`Watched: ${episode.title}`}
      title={watched ? 'Mark unwatched' : 'Mark watched'}
      onClick={() => onSetPlayed([episode], !watched)}
    >
      <WatchedIcon watched={watched} />
    </button>
  )
}

const numberColor = (v: EpisodeView) => (v.hasArt ? 'rgba(255, 255, 255, 0.92)' : '#5C6270')

function EpisodeList({ views, mode, monitored, onToggle, onSetPlayed }: ViewProps) {
  const manage = mode === 'manage'
  const items = []
  let lastSeason: number | null = null
  const seasonOf = (v: EpisodeView) =>
    v.episode.number?.style === 'season' ? (v.episode.number.season ?? null) : null
  for (const v of views) {
    const number = v.episode.number
    if (number?.style === 'season' && number.season != null && number.season !== lastSeason) {
      lastSeason = number.season
      const season = views
        .filter((other) => seasonOf(other) === number.season)
        .map((o) => o.episode)
      const allWatched = season.every(played)
      const name = number.season === 0 ? 'Specials' : `Season ${number.season}`
      items.push(
        <li key={`season-${number.season}`} className={styles.season}>
          {name}
          {!manage && (
            <button
              type="button"
              className={styles.seasonAction}
              onClick={() => onSetPlayed(season, !allWatched)}
            >
              {allWatched ? 'Mark unwatched' : 'Mark watched'}
            </button>
          )}
        </li>,
      )
    }
    const title = v.episode.title
    items.push(
      <li key={v.key} className={`${styles.row} ${v.isUpNext && !manage ? styles.upNext : ''}`}>
        {manage && (
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={monitored(v)}
              onChange={() => onToggle(v.episode)}
              aria-label={`Monitor ${title}`}
            />
          </label>
        )}
        <Thumb v={v} className={styles.thumb}>
          <span className={styles.thumbNumber} style={{ color: numberColor(v) }}>
            {v.label}
          </span>
          <Progress value={v.progress} />
        </Thumb>
        <div className={styles.text}>
          <div className={styles.titleLine}>
            <span className={styles.title}>{title}</span>
            {v.episode.is_filler && <span className={styles.tag}>Filler</span>}
            {manage && v.episode.is_gap && <span className={styles.gap}>Gap</span>}
          </div>
          <span className={styles.meta}>{v.meta}</span>
          {manage && <span className={styles.tech}>{v.tech}</span>}
        </div>
        <Badge badge={v.badge} />
        {!manage && <WatchedToggle episode={v.episode} onSetPlayed={onSetPlayed} />}
        {manage && (
          <button
            type="button"
            className={styles.more}
            aria-label={`More actions for ${title}`}
            title="Episode actions aren't built yet"
            disabled
          >
            <MoreIcon />
          </button>
        )}
      </li>,
    )
  }
  return <ol className={styles.list}>{items}</ol>
}

function EpisodeGrid({ views, mode, monitored, onToggle, onSetPlayed }: ViewProps) {
  const manage = mode === 'manage'
  return (
    <div className={styles.grid}>
      {views.map((v) => (
        <div key={v.key} className={styles.card}>
          <Thumb
            v={v}
            className={`${styles.cardArt} ${v.isUpNext && !manage ? styles.upNext : ''}`}
          >
            <div className={styles.cardTop}>
              <Badge badge={v.badge} small />
              {manage && (
                <label className={styles.cardCheck}>
                  <input
                    type="checkbox"
                    checked={monitored(v)}
                    onChange={() => onToggle(v.episode)}
                  />
                  Monitored
                </label>
              )}
            </div>
            <div>
              <span className={styles.cardNumber} style={{ color: numberColor(v) }}>
                {v.label}
              </span>
              <Progress value={v.progress} />
            </div>
          </Thumb>
          <span className={styles.cardText}>
            <span className={styles.cardTitleLine}>
              <span className={styles.title}>{v.episode.title}</span>
              {!manage && <WatchedToggle episode={v.episode} onSetPlayed={onSetPlayed} />}
            </span>
            <span className={styles.meta}>{v.meta}</span>
            {manage && <span className={styles.tech}>{v.tech}</span>}
          </span>
        </div>
      ))}
    </div>
  )
}
