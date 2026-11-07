import { useState, type ReactNode } from 'react'
import { artworkUrl, type Episode } from '../../api/client'
import { CoverImage } from '../../components/Artwork'
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
  /** Where an episode's own page is. */
  episodeLink: (episode: Episode) => string
  /** Episodes whose monitor change is in flight, with the value being saved. */
  pendingMonitor: Map<number, boolean>
  onToggleMonitor: (episode: Episode) => void
  /** Marks episodes watched or not (watch mode). */
  onSetPlayed: (episodes: Episode[], played: boolean) => void
}

/** A run of episodes from one season; `season` is null when the ordering has none. */
interface Group {
  key: string
  season: number | null
  views: EpisodeView[]
}

function groupBySeason(views: EpisodeView[]): Group[] {
  const groups: Group[] = []
  for (const v of views) {
    const number = v.episode.number
    const season = number?.style === 'season' ? (number.season ?? null) : null
    const last = groups.at(-1)
    if (last && (season === null || last.season === season)) {
      last.views.push(v)
    } else {
      groups.push({ key: `${groups.length}:${season}`, season, views: [v] })
    }
  }
  return groups
}

export function Episodes({
  entries,
  upNext,
  mode,
  layout,
  episodeLink,
  pendingMonitor,
  onToggleMonitor,
  onSetPlayed,
}: Props) {
  /** Seasons opened or closed by hand; the others follow `openByDefault`. */
  const [toggled, setToggled] = useState<Record<string, boolean>>({})
  const views = entries.map((e, i) => episodeView(e, i, upNext, mode))
  const groups = groupBySeason(views)
  const props: ViewProps = {
    mode,
    episodeLink,
    monitored: (v) => pendingMonitor.get(v.episode.item_id) ?? v.episode.monitored,
    onToggle: onToggleMonitor,
    onSetPlayed,
  }
  const body = (g: Group) =>
    layout === 'list' ? (
      <ol className={styles.list}>
        {g.views.map((v) => (
          <EpisodeRow key={v.key} v={v} {...props} />
        ))}
      </ol>
    ) : (
      <div className={styles.grid}>
        {g.views.map((v) => (
          <EpisodeCard key={v.key} v={v} {...props} />
        ))}
      </div>
    )

  if (groups.length <= 1 && groups[0]?.season == null) {
    return groups[0] ? body(groups[0]) : null
  }
  // The season you're in is open; with nothing up next, the first one.
  const current = groups.findIndex((g) => g.views.some((v) => v.isUpNext))
  const openByDefault = (i: number) => groups.length === 1 || i === Math.max(current, 0)
  return (
    <div className={styles.seasons}>
      {groups.map((g, i) => {
        const open = toggled[g.key] ?? openByDefault(i)
        return (
          <section key={g.key} className={styles.seasonGroup}>
            <SeasonHeader
              group={g}
              open={open}
              mode={mode}
              onToggle={() => setToggled((t) => ({ ...t, [g.key]: !open }))}
              onSetPlayed={onSetPlayed}
            />
            {open && body(g)}
          </section>
        )
      })}
    </div>
  )
}

function SeasonHeader({
  group,
  open,
  mode,
  onToggle,
  onSetPlayed,
}: {
  group: Group
  open: boolean
  mode: Mode
  onToggle: () => void
  onSetPlayed: Props['onSetPlayed']
}) {
  const episodes = group.views.map((v) => v.episode)
  const name = group.season === 0 ? 'Specials' : `Season ${group.season}`
  const allWatched = episodes.every(played)
  const summary =
    mode === 'manage'
      ? `${episodes.filter(playable).length} of ${episodes.length} available`
      : `${episodes.filter(played).length} of ${episodes.length} watched`
  return (
    <div className={styles.season}>
      <button type="button" className={styles.seasonToggle} aria-expanded={open} onClick={onToggle}>
        <span className={styles.chevron} aria-hidden="true">
          ›
        </span>
        <span className={styles.seasonName}>{name}</span>
        <span className={styles.seasonSummary}>{summary}</span>
      </button>
      {mode === 'watch' && (
        <button
          type="button"
          className={styles.seasonAction}
          onClick={() => onSetPlayed(episodes, !allWatched)}
        >
          {allWatched ? 'Mark unwatched' : 'Mark watched'}
        </button>
      )}
    </div>
  )
}

interface ViewProps {
  mode: Mode
  episodeLink: (episode: Episode) => string
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
  const still = artworkUrl({ id: v.episode.item_id, artwork: v.episode.artwork }, 'still')
  const picture = still && (
    <span className={styles.still}>
      <CoverImage src={still} />
    </span>
  )
  if (!playable(v.episode)) {
    return (
      <div className={className} style={style}>
        {picture}
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
      {picture}
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

function EpisodeRow({
  v,
  mode,
  episodeLink,
  monitored,
  onToggle,
  onSetPlayed,
}: ViewProps & { v: EpisodeView }) {
  const manage = mode === 'manage'
  const title = v.episode.title
  return (
    <li className={`${styles.row} ${v.isUpNext && !manage ? styles.upNext : ''}`}>
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
          <Link to={episodeLink(v.episode)} className={styles.title}>
            {title}
          </Link>
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
    </li>
  )
}

function EpisodeCard({
  v,
  mode,
  episodeLink,
  monitored,
  onToggle,
  onSetPlayed,
}: ViewProps & { v: EpisodeView }) {
  const manage = mode === 'manage'
  return (
    <div className={styles.card}>
      <Thumb v={v} className={`${styles.cardArt} ${v.isUpNext && !manage ? styles.upNext : ''}`}>
        <div className={styles.cardTop}>
          <Badge badge={v.badge} small />
          {manage && (
            <label className={styles.cardCheck}>
              <input type="checkbox" checked={monitored(v)} onChange={() => onToggle(v.episode)} />
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
          <Link to={episodeLink(v.episode)} className={styles.title}>
            {v.episode.title}
          </Link>
          {!manage && <WatchedToggle episode={v.episode} onSetPlayed={onSetPlayed} />}
        </span>
        <span className={styles.meta}>{v.meta}</span>
        {manage && <span className={styles.tech}>{v.tech}</span>}
      </span>
    </div>
  )
}
