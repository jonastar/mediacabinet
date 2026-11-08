import { useState, type ReactNode } from 'react'
import { artworkUrl, type Episode } from '../../api/client'
import { CoverImage } from '../../components/Artwork'
import { Badge } from '../../components/Badge'
import { Countdown } from '../../components/Countdown'
import {
  AirDateIcon,
  CheckIcon,
  ChevronDownIcon,
  GridIcon,
  ListIcon,
  PlayIcon,
  SelectIcon,
  WatchedIcon,
} from '../../components/Icons'
import { Link } from '../../components/Link'
import { Progress } from '../../components/Progress'
import { Segmented } from '../../components/Segmented'
import { airDayLabel } from '../../lib/format'
import { useEpisodeLayout } from '../../lib/prefs'
import { episodeView, groupBySeason, type EpisodeView, type SeasonGroup } from './episodeView'
import styles from './Episodes.module.css'

const played = (e: Episode) => e.watch?.played ?? false
const playable = (e: Episode) => e.availability === 'available'
const isUpcoming = (e: Episode) => e.availability === 'upcoming'

/** The next episode to air in a season, for its badge. */
const nextToAir = (episodes: Episode[]) =>
  episodes
    .filter((e) => isUpcoming(e) && e.airs_at != null)
    .sort((a, b) => (a.airs_at ?? 0) - (b.airs_at ?? 0))[0]

interface Props {
  entries: Episode[]
  upNext: number | null | undefined
  /** Where an episode's own page is. */
  episodeLink: (episode: Episode) => string
  onSetPlayed: (episodes: Episode[], played: boolean) => void
  /** Starts a playlist from the selected episodes. */
  onAddToPlaylist: (episodes: Episode[]) => void
  /** The ordering picker, and a line about the ordering. */
  order: ReactNode
  /** The season to show first, when a link asked for one. */
  initialSeason: string | null
  note: ReactNode
  /** Shown instead of the episodes while they load or if they failed. */
  status?: ReactNode
}

/** The Episodes section in viewing mode: season tabs, collapsible seasons and Select mode. */
export function Episodes({
  entries,
  upNext,
  episodeLink,
  onSetPlayed,
  onAddToPlaylist,
  order,
  initialSeason,
  note,
  status,
}: Props) {
  const [layout, setLayout] = useEpisodeLayout()
  /** Seasons opened or closed by hand; the others follow the default. */
  const [toggled, setToggled] = useState<Record<string, boolean>>({})
  const [selecting, setSelecting] = useState(false)
  const [selected, setSelected] = useState(new Set<number>())

  const groups = groupBySeason(entries.map((e, i) => episodeView(e, i, upNext, 'watch')))
  // A season a link asked for is open; otherwise the one you're in, or the first.
  const linked = groups.findIndex((g) => g.season !== null && String(g.season) === initialSeason)
  const current = groups.findIndex((g) => g.views.some((v) => v.isUpNext))
  const opened = linked >= 0 ? linked : Math.max(current, 0)
  const openByDefault = (g: SeasonGroup) => groups.length === 1 || groups.indexOf(g) === opened

  const selectable = (list: SeasonGroup[]) =>
    list.flatMap((g) => g.views.map((v) => v.episode)).filter((e) => !isUpcoming(e))
  const setChosen = (ids: number[], on: boolean) => {
    setSelected((current) => {
      const next = new Set(current)
      for (const id of ids) {
        if (on) {
          next.add(id)
        } else {
          next.delete(id)
        }
      }
      return next
    })
  }
  const chosen = entries.filter((e) => selected.has(e.item_id))
  const stopSelecting = () => {
    setSelecting(false)
    setSelected(new Set())
  }

  return (
    <section aria-labelledby="episodes-title" className={styles.section}>
      <div className={styles.toolbar}>
        <div className={styles.toolbarStart}>
          <h2 id="episodes-title">Episodes</h2>
        </div>
        <div className={styles.toolbarEnd}>
          {order}
          <Segmented
            label="Layout"
            variant="box"
            value={layout}
            onChange={setLayout}
            options={[
              { value: 'list', label: 'List', icon: <ListIcon /> },
              { value: 'grid', label: 'Grid', icon: <GridIcon /> },
            ]}
          />
          <button
            type="button"
            className={`${styles.tool} ${selecting ? styles.toolOn : ''}`}
            aria-pressed={selecting}
            onClick={() => (selecting ? stopSelecting() : setSelecting(true))}
          >
            <SelectIcon />
            Select
          </button>
        </div>
      </div>
      {note}

      {selecting && (
        <div className={styles.selectionBar} role="toolbar" aria-label="Selected episodes">
          <div className={styles.selectionStart}>
            <span className={styles.selectionCount} aria-live="polite">
              {chosen.length} selected
            </span>
            <button
              type="button"
              className={styles.selectAll}
              onClick={() =>
                setChosen(
                  selectable(groups).map((e) => e.item_id),
                  true,
                )
              }
            >
              Select all
            </button>
            <button type="button" className={styles.clear} onClick={() => setSelected(new Set())}>
              Clear
            </button>
          </div>
          <div className={styles.selectionActions}>
            <button
              type="button"
              className={styles.barAction}
              disabled={chosen.length === 0}
              onClick={() => onSetPlayed(chosen, true)}
            >
              <CheckIcon />
              Mark watched
            </button>
            <button
              type="button"
              className={styles.barAction}
              disabled={chosen.length === 0}
              onClick={() => onSetPlayed(chosen, false)}
            >
              Mark unwatched
            </button>
            <button
              type="button"
              className={styles.barAction}
              disabled={chosen.length === 0}
              onClick={() => onAddToPlaylist(chosen)}
            >
              Add to playlist
            </button>
            <button type="button" className={styles.done} onClick={stopSelecting}>
              Done
            </button>
          </div>
        </div>
      )}

      {status ?? (
        <div className={styles.groups}>
          {groups.map((g) => {
            const collapsible = groups.length > 1
            const open = !collapsible || (toggled[g.key] ?? openByDefault(g))
            const grid = layout === 'grid'
            const Upcoming = grid ? UpcomingCard : UpcomingRow
            const Available = grid ? EpisodeCard : EpisodeRow
            const rows = (
              <ol className={grid ? styles.grid : styles.list}>
                {g.views.map((v) =>
                  isUpcoming(v.episode) ? (
                    <Upcoming key={v.key} v={v} />
                  ) : (
                    <Available
                      key={v.key}
                      v={v}
                      episodeLink={episodeLink}
                      selecting={selecting}
                      checked={selected.has(v.episode.item_id)}
                      onToggle={(on) => setChosen([v.episode.item_id], on)}
                      onSetPlayed={onSetPlayed}
                    />
                  ),
                )}
              </ol>
            )
            if (g.season === null) {
              return <div key={g.key}>{rows}</div>
            }
            return (
              <section key={g.key} id={`season-${g.season}`} className={styles.group}>
                <SeasonHeader
                  group={g}
                  open={open}
                  collapsible={collapsible}
                  selecting={selecting}
                  allChosen={selectable([g]).every((e) => selected.has(e.item_id))}
                  onChoose={(on) =>
                    setChosen(
                      selectable([g]).map((e) => e.item_id),
                      on,
                    )
                  }
                  onToggle={() => setToggled((t) => ({ ...t, [g.key]: !open }))}
                  onSetPlayed={onSetPlayed}
                />
                {open && rows}
              </section>
            )
          })}
        </div>
      )}
    </section>
  )
}

function SeasonHeader({
  group,
  open,
  collapsible,
  selecting,
  allChosen,
  onChoose,
  onToggle,
  onSetPlayed,
}: {
  group: SeasonGroup
  open: boolean
  collapsible: boolean
  selecting: boolean
  allChosen: boolean
  onChoose: (on: boolean) => void
  onToggle: () => void
  onSetPlayed: Props['onSetPlayed']
}) {
  const all = group.views.map((v) => v.episode)
  const aired = all.filter((e) => !isUpcoming(e))
  const watched = aired.filter(played).length
  const upcoming = all.length - aired.length
  const allWatched = aired.length > 0 && watched === aired.length
  const next = nextToAir(all)
  const summary = [
    `${watched} of ${aired.length} watched`,
    upcoming > 0 ? `${upcoming} upcoming` : null,
  ]
    .filter(Boolean)
    .join(' · ')
  return (
    <div className={styles.seasonHeader}>
      <div className={styles.seasonStart}>
        {selecting && (
          <input
            type="checkbox"
            className={styles.seasonCheck}
            checked={aired.length > 0 && allChosen}
            disabled={aired.length === 0}
            aria-label={`Select all of ${group.name}`}
            onChange={() => onChoose(!allChosen)}
          />
        )}
        {collapsible ? (
          <button
            type="button"
            className={styles.seasonToggle}
            aria-expanded={open}
            onClick={onToggle}
          >
            <span className={styles.chevron}>
              <ChevronDownIcon />
            </span>
            {group.name}
          </button>
        ) : (
          <h3 className={styles.seasonName}>{group.name}</h3>
        )}
        <span className={styles.seasonBar} aria-hidden="true">
          <span style={{ width: `${aired.length ? (watched / aired.length) * 100 : 0}%` }} />
        </span>
        <span className={styles.seasonSummary}>{summary}</span>
        {next?.airs_at != null && (
          <span className={styles.nextBadge}>
            Next <Countdown airsAt={next.airs_at} timeKnown={next.airs_time_known} short />
          </span>
        )}
      </div>
      {!selecting && aired.length > 0 && (
        <button
          type="button"
          className={styles.seasonAction}
          onClick={() => onSetPlayed(aired, !allWatched)}
        >
          {allWatched ? 'Mark season unwatched' : 'Mark season watched'}
        </button>
      )}
    </div>
  )
}

/** The picture, with the episode's number on it; plays the episode when there's a file. */
function Still({
  v,
  selecting,
  className,
  children,
}: {
  v: EpisodeView
  selecting: boolean
  className: string
  children: ReactNode
}) {
  const style = { background: v.tone }
  const still = artworkUrl({ id: v.episode.item_id, artwork: v.episode.artwork }, 'still')
  const contents = (
    <>
      {still && (
        <span className={styles.stillImage}>
          <CoverImage src={still} />
        </span>
      )}
      {children}
      {v.progress > 0 && (
        <span className={styles.stillProgress}>
          <Progress value={v.progress} />
        </span>
      )}
    </>
  )
  if (selecting || !playable(v.episode)) {
    return (
      <span className={className} style={style}>
        {contents}
      </span>
    )
  }
  return (
    <Link
      to={`/play/${v.episode.item_id}`}
      className={`${className} ${styles.playable}`}
      style={style}
      aria-label={`Play ${v.episode.title}`}
    >
      {contents}
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
  onSetPlayed: Props['onSetPlayed']
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

/** Light over a picture or a tinted placeholder; dim on an empty one. */
const numberColor = (v: EpisodeView) =>
  v.hasArt || v.episode.artwork.includes('still') ? 'rgba(255, 255, 255, 0.92)' : '#5C6270'

function EpisodeTitle({ v, episodeLink, selecting }: EpisodeProps) {
  const e = v.episode
  return selecting ? (
    <span className={styles.title}>{e.title}</span>
  ) : (
    <Link to={episodeLink(e)} className={styles.title}>
      {e.title}
    </Link>
  )
}

interface EpisodeProps {
  v: EpisodeView
  episodeLink: Props['episodeLink']
  selecting: boolean
  checked: boolean
  onToggle: (on: boolean) => void
  onSetPlayed: Props['onSetPlayed']
}

function EpisodeRow(props: EpisodeProps) {
  const { v, selecting, checked, onToggle, onSetPlayed } = props
  const e = v.episode
  const wash = selecting ? checked : v.isUpNext
  const className = `${styles.row} ${wash ? styles.wash : ''}`
  const contents = (
    <>
      {selecting && (
        <input
          type="checkbox"
          className={styles.rowCheck}
          checked={checked}
          aria-label={`Select ${v.label}`}
          onChange={() => onToggle(!checked)}
        />
      )}
      <Still v={v} selecting={selecting} className={styles.still}>
        <span className={styles.stillNumber} style={{ color: numberColor(v) }}>
          {v.label}
        </span>
      </Still>
      <span className={styles.text}>
        <span className={styles.titleLine}>
          <EpisodeTitle {...props} />
          {e.is_filler && <span className={styles.tag}>Filler</span>}
        </span>
        <span className={styles.meta}>{v.meta}</span>
        {e.overview && <span className={styles.overview}>{e.overview}</span>}
      </span>
      <Badge badge={v.badge} />
      {!selecting && <WatchedToggle episode={e} onSetPlayed={onSetPlayed} />}
    </>
  )
  // While selecting, anywhere on the row toggles it.
  return (
    <li>
      {selecting ? (
        <label className={className}>{contents}</label>
      ) : (
        <div className={className}>{contents}</div>
      )}
    </li>
  )
}

function EpisodeCard(props: EpisodeProps) {
  const { v, selecting, checked, onToggle, onSetPlayed } = props
  const e = v.episode
  const lit = selecting ? checked : v.isUpNext
  const contents = (
    <>
      <Still
        v={v}
        selecting={selecting}
        className={`${styles.cardArt} ${lit ? styles.cardLit : ''}`}
      >
        <span className={styles.cardTop}>
          {selecting && (
            <input
              type="checkbox"
              className={styles.rowCheck}
              checked={checked}
              aria-label={`Select ${v.label}`}
              onChange={() => onToggle(!checked)}
            />
          )}
          <span className={styles.cardBadge}>
            <Badge badge={v.badge} small />
          </span>
        </span>
        <span className={styles.cardNumber} style={{ color: numberColor(v) }}>
          {v.label}
        </span>
      </Still>
      <span className={styles.cardText}>
        <span className={styles.cardTitleLine}>
          <span className={styles.titleLine}>
            <EpisodeTitle {...props} />
            {e.is_filler && <span className={styles.tag}>Filler</span>}
          </span>
          {!selecting && <WatchedToggle episode={e} onSetPlayed={onSetPlayed} />}
        </span>
        <span className={styles.meta}>{v.meta}</span>
      </span>
    </>
  )
  return (
    <li>
      {selecting ? (
        <label className={`${styles.card} ${styles.cardPick}`}>{contents}</label>
      ) : (
        <div className={styles.card}>{contents}</div>
      )}
    </li>
  )
}

const airsLine = (e: Episode) =>
  e.airs_at != null ? `Airs ${airDayLabel(e.airs_at, e.airs_time_known)}` : 'No air date yet'

/** An episode that hasn't aired: nothing to play or select yet. */
function UpcomingRow({ v }: { v: EpisodeView }) {
  const e = v.episode
  return (
    <li className={`${styles.row} ${styles.upcoming}`}>
      <span className={styles.upcomingStill}>
        <AirDateIcon />
      </span>
      <span className={styles.text}>
        <span className={styles.titleLine}>
          <span className={styles.code}>{v.label}</span>
          <span className={`${styles.title} ${styles.upcomingTitle}`}>{e.title}</span>
        </span>
        <span className={styles.meta}>{airsLine(e)}</span>
      </span>
      {e.airs_at != null && (
        <span className={styles.countdown}>
          <Countdown airsAt={e.airs_at} timeKnown={e.airs_time_known} short />
        </span>
      )}
    </li>
  )
}

function UpcomingCard({ v }: { v: EpisodeView }) {
  const e = v.episode
  return (
    <li className={styles.card}>
      <span className={`${styles.cardArt} ${styles.upcoming} ${styles.upcomingArt}`}>
        <span className={styles.cardTop}>
          {e.airs_at != null && (
            <span className={styles.countdown}>
              <Countdown airsAt={e.airs_at} timeKnown={e.airs_time_known} short />
            </span>
          )}
        </span>
        <AirDateIcon />
        <span className={styles.cardNumber}>{v.label}</span>
      </span>
      <span className={styles.cardText}>
        <span className={`${styles.title} ${styles.upcomingTitle}`}>{e.title}</span>
        <span className={styles.meta}>{airsLine(e)}</span>
      </span>
    </li>
  )
}
