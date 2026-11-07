import { episodeLabel, episodeLine } from '@core/episode'
import { useEffect, useRef, useState } from 'preact/hooks'
import type { Screen } from '../App'
import {
  api,
  mediaToken,
  mediaUrl,
  type Episode,
  type EpisodeList,
  type SeriesDetail,
} from '../api'
import { returningKey, useInitialFocus, usingPointer } from '../focus'
import { Rail } from './Rail'

/** A row and the space under it; `.episode-row` has the height without the space. */
const ROW_HEIGHT = 180
/** How many rows exist around the ones in use, on the way past the screen's edges. */
const ROWS_BEFORE = 3
const ROWS_AFTER = 8

interface Season {
  name: string | null
  episodes: Episode[]
}

/** Runs of episodes from one season, or a single run when the ordering has none. */
function seasonsOf(episodes: Episode[]): Season[] {
  const seasons: Array<Season & { number: number | null }> = []
  for (const episode of episodes) {
    const number = episode.number?.style === 'season' ? (episode.number.season ?? null) : null
    const last = seasons[seasons.length - 1]
    if (last && last.number === number) {
      last.episodes.push(episode)
    } else {
      let name = null
      if (number !== null) {
        name = number === 0 ? 'Specials' : `Season ${number}`
      }
      seasons.push({ number, name, episodes: [episode] })
    }
  }
  return seasons
}

const playableIn = (list: EpisodeList) => list.entries.filter((entry) => entry.files.length > 0)

/** Picks out the episode up next in a list. */
const isNext = (list: EpisodeList) => (episode: Episode) =>
  list.up_next != null && list.entries[list.up_next] === episode

const isStarted = (episode: Episode) =>
  !episode.watch?.played && (episode.watch?.position_seconds ?? 0) > 0

function EpisodeRow({
  episode,
  top,
  upNext,
  image,
  onFocus,
  onSelect,
}: {
  episode: Episode
  top: number
  upNext: boolean
  image: string | null
  onFocus: () => void
  onSelect: () => void
}) {
  const position = episode.watch?.position_seconds ?? 0
  const percent =
    isStarted(episode) && episode.runtime_seconds
      ? Math.min(100, (position / episode.runtime_seconds) * 100)
      : null
  let state = null
  if (upNext) {
    state = 'Up next'
  } else if (episode.watch?.played) {
    state = 'Watched'
  }
  return (
    <button
      className="episode-row"
      style={{ top: `${top}px` }}
      data-key={`episode-${episode.item_id}`}
      data-default={upNext ? '' : undefined}
      onFocus={onFocus}
      onClick={onSelect}
    >
      <span className="episode-still">
        {image && <img src={image} alt="" />}
        {percent !== null && (
          <span className="card-progress">
            <span style={{ width: `${percent}%` }} />
          </span>
        )}
      </span>
      <span className="episode-text">
        <span className="episode-title">{episodeLine(episode)}</span>
        {episode.overview && <span className="episode-about">{episode.overview}</span>}
      </span>
      {state && <span className={upNext ? 'episode-state next' : 'episode-state'}>{state}</span>}
    </button>
  )
}

/** A show: what it is, the way to carry on with it, and its episodes by season. */
export function Series({
  screen,
  open,
}: {
  screen: { seriesId: number; title: string }
  open: (screen: Screen) => void
}) {
  const [detail, setDetail] = useState<SeriesDetail | null>(null)
  const [list, setList] = useState<EpisodeList | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const [season, setSeason] = useState(0)
  /** The row focus is on or last went to, which has to stay in existence. */
  const [focusRow, setFocusRow] = useState(0)
  /** The row at the top of the screen, for a pointer scrolling away from focus. */
  const [topRow, setTopRow] = useState(0)
  const rowsRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    let current = true
    Promise.all([api.series(screen.seriesId), api.episodes(screen.seriesId), mediaToken()]).then(
      ([loadedDetail, loadedList, media]) => {
        if (current) {
          // Open where focus is coming back to, or else at the episode up next.
          const returning = returningKey()
          const rows = seasonsOf(playableIn(loadedList))
          const wanted = (episode: Episode) => `episode-${episode.item_id}` === returning
          const start = returning?.indexOf('episode-') === 0 ? wanted : isNext(loadedList)
          const index = rows.findIndex((s) => s.episodes.some(start))
          if (index >= 0) {
            setSeason(index)
            setFocusRow(rows[index].episodes.findIndex(start))
          }
          setDetail(loadedDetail)
          setList(loadedList)
          setToken(media)
        }
      },
      (err: Error) => {
        if (current) {
          setProblem(err.message)
        }
      },
    )
    return () => {
      current = false
    }
  }, [screen.seriesId])

  const playable = list ? playableIn(list) : []
  const seasons = seasonsOf(playable)
  const main = (list && playable.filter(isNext(list))[0]) ?? playable[0] ?? null
  useInitialFocus(list ?? problem)

  const selectSeason = (index: number) => {
    setSeason(index)
    setFocusRow(main ? Math.max(0, seasons[index].episodes.indexOf(main)) : 0)
  }

  const onScroll = () => {
    const top = rowsRef.current?.getBoundingClientRect().top ?? 0
    setTopRow(Math.max(0, Math.floor(-top / ROW_HEIGHT)))
  }

  const near = (row: number, around: number) =>
    row >= around - ROWS_BEFORE && row <= around + ROWS_AFTER

  const playEpisode = (episode: Episode) =>
    open({ name: 'player', itemId: episode.item_id, title: `${screen.title}: ${episode.title}` })

  const media = (id: number, kind: string) =>
    token ? mediaUrl(`/api/artwork/${id}/${kind}`, token) : null
  const backdrop = detail?.artwork.indexOf('backdrop') !== -1 ? 'backdrop' : 'poster'
  const shown = seasons[season]?.episodes ?? []
  const facts = detail
    ? [detail.year, detail.genres.slice(0, 3).join(', '), `${playable.length} episodes`]
    : []

  return (
    <main className="series" onScroll={onScroll}>
      <header className="series-hero">
        {detail && detail.artwork.length > 0 && (
          <img className="series-backdrop" src={media(detail.id, backdrop) ?? undefined} alt="" />
        )}
        <h1>{screen.title}</h1>
        {detail && <p className="series-meta">{facts.filter(Boolean).join(' · ')}</p>}
        {detail?.overview && <p className="series-overview">{detail.overview}</p>}
        {problem && (
          <p className="problem" role="alert">
            {problem}
          </p>
        )}
        {main && (
          <button
            className="series-play"
            data-first=""
            data-key="play"
            onClick={() => playEpisode(main)}
          >
            {isStarted(main) ? 'Resume' : 'Play'} {episodeLabel(main.number)}
          </button>
        )}
      </header>
      {list && playable.length === 0 && (
        <p className="muted series-empty">No episodes to play yet.</p>
      )}
      {seasons.length > 1 && (
        <Rail name="seasons" className="seasons">
          {seasons.map((s, index) => (
            <button
              key={index}
              className={index === season ? 'season selected' : 'season'}
              data-default={index === season ? '' : undefined}
              onFocus={() => {
                // A pointer passes over the tabs on its way to the rows, so it has to click.
                if (index !== season && !usingPointer()) {
                  selectSeason(index)
                }
              }}
              onClick={() => selectSeason(index)}
            >
              {s.name}
            </button>
          ))}
        </Rail>
      )}
      <div
        className="episodes"
        ref={rowsRef}
        data-rail="episodes"
        style={{ height: `${shown.length * ROW_HEIGHT}px` }}
      >
        {shown.map((episode, row) =>
          near(row, focusRow) || near(row, topRow) ? (
            <EpisodeRow
              key={episode.item_id}
              episode={episode}
              top={row * ROW_HEIGHT}
              upNext={list !== null && isNext(list)(episode)}
              image={episode.artwork.indexOf('still') >= 0 ? media(episode.item_id, 'still') : null}
              onFocus={() => setFocusRow(row)}
              onSelect={() => playEpisode(episode)}
            />
          ) : null,
        )}
      </div>
    </main>
  )
}
