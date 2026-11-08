import { episodeLine } from '@core/episode'
import { progress } from '@core/progress'
import { useEffect, useState } from 'preact/hooks'
import type { Screen } from '../App'
import { api, hosted, mediaToken, mediaUrl, type Home } from '../api'
import { useInitialFocus } from '../focus'
import { Rail } from './Rail'

type ContinueItem = Home['keep_watching'][number]

function Card({
  title,
  detail,
  image,
  first,
  focusKey,
  landscape,
  watched,
  onSelect,
}: {
  title: string
  detail?: string
  image: string | null
  first?: boolean
  focusKey: string
  landscape?: boolean
  /** How far through it someone is, from 0 to 1. */
  watched?: number
  onSelect: () => void
}) {
  return (
    <button
      className={landscape ? 'card landscape' : 'card'}
      data-first={first ? '' : undefined}
      data-key={focusKey}
      onClick={onSelect}
    >
      <span className="poster">
        {image && <img src={image} alt="" />}
        {!!watched && (
          <span className="card-progress">
            <span style={{ width: `${watched * 100}%` }} />
          </span>
        )}
      </span>
      <span className="card-title">{title}</span>
      {detail && <span className="card-detail">{detail}</span>}
    </button>
  )
}

/** What's on the server: keep watching, shows and movies. */
export function Browse({
  open,
  onChangeServer,
}: {
  open: (screen: Screen) => void
  onChangeServer: () => void
}) {
  const [home, setHome] = useState<Home | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  useInitialFocus(home ?? problem)

  useEffect(() => {
    let current = true
    setProblem(null)
    Promise.all([api.home(), mediaToken()]).then(
      ([loaded, media]) => {
        if (current) {
          setToken(media)
          setHome(loaded)
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
  }, [attempt])

  const artwork = (id: number, kinds: string[], wanted: string[]) => {
    const kind = wanted.filter((k) => kinds.indexOf(k) >= 0)[0] ?? kinds[0]
    return kind && token ? mediaUrl(`/api/artwork/${id}/${kind}`, token) : null
  }
  const poster = (id: number, kinds: string[]) => artwork(id, kinds, ['poster'])

  /** The episode's own picture, or else the show's. */
  const episodeImage = ({ episode, series_id }: ContinueItem) => {
    // A server older than this app doesn't send `artwork` on episodes.
    if (episode.artwork?.length) {
      return artwork(episode.item_id, episode.artwork, ['still'])
    }
    const show = home?.shows.filter((s) => s.id === series_id)[0]
    return show ? artwork(show.id, show.artwork, ['backdrop', 'poster']) : null
  }

  const leaveLabel = hosted ? 'Leave TV mode' : 'Change server'
  const empty = home && !home.keep_watching.length && !home.shows.length && !home.movies.length

  return (
    <main className="browse">
      {problem && (
        <div className="center">
          <h1>Couldn't load your library</h1>
          <p className="problem" role="alert">
            {problem}
          </p>
          <div className="actions">
            <button onClick={() => setAttempt((n) => n + 1)}>Try again</button>
            <button onClick={onChangeServer}>{leaveLabel}</button>
          </div>
        </div>
      )}
      {!problem && !home && <p className="muted center">Loading…</p>}
      {empty && <p className="muted">There's nothing to watch here yet.</p>}
      {home && home.keep_watching.length > 0 && (
        <Rail name="keep-watching" title="Keep watching">
          {home.keep_watching.map((item, i) => (
            <Card
              key={item.episode.item_id}
              first={i === 0}
              focusKey={`continue-${item.series_id}`}
              landscape
              title={item.series_title}
              detail={episodeLine(item.episode)}
              image={episodeImage(item)}
              watched={progress(item.episode.runtime_seconds, item.position_seconds, false)}
              onSelect={() =>
                open({ name: 'player', itemId: item.episode.item_id, title: item.series_title })
              }
            />
          ))}
        </Rail>
      )}
      {home && home.shows.length > 0 && (
        <Rail name="shows" title="Shows">
          {home.shows.map((show) => (
            <Card
              key={show.id}
              focusKey={`series-${show.id}`}
              title={show.title}
              image={poster(show.id, show.artwork)}
              onSelect={() => open({ name: 'series', seriesId: show.id, title: show.title })}
            />
          ))}
        </Rail>
      )}
      {home && home.movies.length > 0 && (
        <Rail name="movies" title="Movies">
          {home.movies.map((movie) => (
            <Card
              key={movie.id}
              focusKey={`movie-${movie.id}`}
              title={movie.title}
              image={poster(movie.id, movie.artwork)}
              onSelect={() => open({ name: 'player', itemId: movie.id, title: movie.title })}
            />
          ))}
        </Rail>
      )}
      {home && (
        <div className="actions">
          <button data-key="about" onClick={() => open({ name: 'diagnostics' })}>
            {hosted ? 'About this browser' : 'About this TV'}
          </button>
          <button onClick={onChangeServer}>{leaveLabel}</button>
        </div>
      )}
    </main>
  )
}
