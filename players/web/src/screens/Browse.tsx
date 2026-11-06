import { episodeLine } from '@core/episode'
import { useEffect, useState } from 'preact/hooks'
import type { Screen } from '../App'
import { api, mediaToken, mediaUrl, type Home } from '../api'
import { useInitialFocus } from '../focus'

type ContinueItem = Home['keep_watching'][number]

/** How far through an episode someone is, as a percentage, once they've started it. */
function percentWatched({ episode, position_seconds: position }: ContinueItem) {
  if (!episode.runtime_seconds || position <= 0) {
    return null
  }
  return Math.min(100, (position / episode.runtime_seconds) * 100)
}

function Card({
  title,
  detail,
  image,
  first,
  landscape,
  percent,
  onSelect,
}: {
  title: string
  detail?: string
  image: string | null
  first?: boolean
  landscape?: boolean
  percent?: number | null
  onSelect: () => void
}) {
  return (
    <button
      className={landscape ? 'card landscape' : 'card'}
      data-first={first ? '' : undefined}
      onClick={onSelect}
    >
      <span className="poster">
        {image && <img src={image} alt="" />}
        {percent != null && (
          <span className="card-progress">
            <span style={{ width: `${percent}%` }} />
          </span>
        )}
      </span>
      <span className="card-title">{title}</span>
      {detail && <span className="card-detail">{detail}</span>}
    </button>
  )
}

function Rail({ title, children }: { title: string; children: preact.ComponentChildren }) {
  return (
    <section className="rail">
      <h2>{title}</h2>
      <div className="rail-items">{children}</div>
    </section>
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
            <button onClick={onChangeServer}>Change server</button>
          </div>
        </div>
      )}
      {!problem && !home && <p className="muted center">Loading…</p>}
      {empty && <p className="muted">There's nothing to watch here yet.</p>}
      {home && home.keep_watching.length > 0 && (
        <Rail title="Keep watching">
          {home.keep_watching.map((item, i) => (
            <Card
              key={item.episode.item_id}
              first={i === 0}
              landscape
              title={item.series_title}
              detail={episodeLine(item.episode)}
              image={episodeImage(item)}
              percent={percentWatched(item)}
              onSelect={() =>
                open({ name: 'player', itemId: item.episode.item_id, title: item.series_title })
              }
            />
          ))}
        </Rail>
      )}
      {home && home.shows.length > 0 && (
        <Rail title="Shows">
          {home.shows.map((show) => (
            <Card
              key={show.id}
              title={show.title}
              image={poster(show.id, show.artwork)}
              onSelect={() => open({ name: 'episodes', seriesId: show.id, title: show.title })}
            />
          ))}
        </Rail>
      )}
      {home && home.movies.length > 0 && (
        <Rail title="Movies">
          {home.movies.map((movie) => (
            <Card
              key={movie.id}
              title={movie.title}
              image={poster(movie.id, movie.artwork)}
              onSelect={() => open({ name: 'player', itemId: movie.id, title: movie.title })}
            />
          ))}
        </Rail>
      )}
      {home && (
        <div className="actions">
          <button onClick={() => open({ name: 'diagnostics' })}>About this TV</button>
          <button onClick={onChangeServer}>Change server</button>
        </div>
      )}
    </main>
  )
}
