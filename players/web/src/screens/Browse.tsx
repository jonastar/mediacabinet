import { useEffect, useState } from 'preact/hooks'
import type { Screen } from '../App'
import { api, mediaToken, mediaUrl, type Home } from '../api'
import { useInitialFocus } from '../focus'

function Card({
  title,
  detail,
  image,
  first,
  onSelect,
}: {
  title: string
  detail?: string
  image: string | null
  first?: boolean
  onSelect: () => void
}) {
  return (
    <button className="card" data-first={first ? '' : undefined} onClick={onSelect}>
      <span className="poster">{image && <img src={image} alt="" />}</span>
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

  const poster = (id: number, kinds: string[]) => {
    const kind = kinds.indexOf('poster') >= 0 ? 'poster' : kinds[0]
    return kind && token ? mediaUrl(`/api/artwork/${id}/${kind}`, token) : null
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
              title={item.series_title}
              detail={item.episode.title}
              image={null}
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
