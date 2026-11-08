import { useState } from 'react'
import {
  api,
  artworkUrl,
  type ActivityEntry,
  type ComingUp,
  type ContinueItem,
  type Home,
  type ItemRef,
} from '../api/client'
import { heroBackground } from '../lib/artwork'
import { useLiveEvents } from '../lib/live'
import { itemPath, navigate } from '../lib/router'
import { useApi } from '../api/useApi'
import { PlayIcon } from '../components/Icons'
import { CoverImage } from '../components/Artwork'
import { MoviePoster, Poster } from '../components/Poster'
import { Progress } from '../components/Progress'
import { Failed, Loading } from '../components/Status'
import { ago, airDay, episodeLabel, minutesLeft, progress, seriesTone } from '../lib/format'
import { useMode } from '../lib/prefs'
import { Link } from '../components/Link'
import { ActivityList } from '../components/ActivityFeed'
import { useUser } from '../lib/session'
import styles from './HomePage.module.css'
import { LOCALE } from '../core/locale'

const RAIL_LIMIT = 12
const ADDED_LIMIT = 12

const headerDate = new Intl.DateTimeFormat(LOCALE, {
  weekday: 'long',
  day: 'numeric',
  month: 'long',
})

function episodeLine(c: ContinueItem) {
  return `${episodeLabel(c.episode.number)} · ${c.episode.title}`
}

/** `49 min left`, or how far into the series this episode is. */
function leftLine(c: ContinueItem) {
  const left = minutesLeft(c.episode.runtime_seconds, c.position_seconds)
  if (left) {
    return `${left} min left`
  }
  const n = c.episode.number
  return n?.season === 1 && n.number === 1 ? 'Start series' : 'Up next'
}

export function HomePage() {
  const [mode] = useMode()
  const home = useApi('home', api.home)
  useLiveEvents(
    [
      'file.added',
      'file.missing',
      'link.created',
      'item.added',
      'item.wanted',
      'item.unwanted',
      'match.needs_review',
      'status.changed',
      'scan.finished',
    ],
    home.reload,
    1000,
  )
  if (home.error) {
    return <Failed error={home.error} onRetry={home.reload} />
  }
  if (!home.data) {
    return <Loading />
  }
  const data = home.data
  return (
    <div className={styles.page}>
      <Header />
      <UpNext home={data} />
      <ThisWeek home={data} />
      <RecentlyAdded home={data} />
      <FriendsWatching />
      {data.movies.length > 0 && (
        <section aria-labelledby="home-movies" className={styles.section}>
          <div className={styles.sectionHead}>
            <h2 id="home-movies">Movies</h2>
            {data.movies.length > RAIL_LIMIT && (
              <Link to="/movies" className={styles.seeAll}>
                See all {data.movies.length}
              </Link>
            )}
          </div>
          <div className={`${styles.rail} ${styles.posterRail}`}>
            {data.movies.slice(0, RAIL_LIMIT).map((m) => (
              <MoviePoster key={m.id} movie={m} mode={mode} />
            ))}
          </div>
        </section>
      )}
      <section aria-labelledby="home-tv" className={styles.section}>
        <div className={styles.sectionHead}>
          <h2 id="home-tv">TV Shows</h2>
          {data.shows.length > RAIL_LIMIT && (
            <Link to="/tv" className={styles.seeAll}>
              See all {data.shows.length}
            </Link>
          )}
        </div>
        {data.shows.length === 0 ? (
          <p className={styles.empty}>No shows yet. Add a library to get started.</p>
        ) : (
          <div className={`${styles.rail} ${styles.posterRail}`}>
            {data.shows.slice(0, RAIL_LIMIT).map((s) => (
              <Poster key={s.id} series={s} mode={mode} />
            ))}
          </div>
        )}
      </section>
    </div>
  )
}

function Header() {
  const [query, setQuery] = useState('')
  const [now] = useState(() => new Date())
  return (
    <header className={styles.header}>
      <h1 className={styles.eyebrow}>{headerDate.format(now)}</h1>
      <form
        role="search"
        className={styles.searchForm}
        onSubmit={(e) => {
          e.preventDefault()
          if (query.trim()) {
            navigate(`/search?q=${encodeURIComponent(query.trim())}`)
          }
        }}
      >
        <label htmlFor="home-search" className="visually-hidden">
          Search
        </label>
        <input
          id="home-search"
          type="search"
          className={styles.search}
          placeholder="Search shows, movies, episodes…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </form>
    </header>
  )
}

function backdropOf(home: Home, seriesId: number) {
  const show = home.shows.find((s) => s.id === seriesId)
  return show ? artworkUrl(show, 'backdrop') : null
}

/** The next episode of each series in progress, most recently watched first. */
function UpNext({ home }: { home: Home }) {
  return (
    <section aria-labelledby="home-up-next" className={styles.section}>
      <div className={styles.sectionHead}>
        <h2 id="home-up-next">Up next</h2>
      </div>
      {home.keep_watching.length > 0 ? (
        <div className={`${styles.rail} ${styles.upNextRail}`}>
          {home.keep_watching.slice(0, RAIL_LIMIT).map((c) => (
            <UpNextCard key={c.series_id} item={c} home={home} />
          ))}
        </div>
      ) : (
        <p className={styles.muted}>Nothing in progress. Pick something from your shows below.</p>
      )}
    </section>
  )
}

function UpNextCard({ item: c, home }: { item: ContinueItem; home: Home }) {
  return (
    <div className={styles.upNextCard}>
      <Link
        to={`/tv/${c.series_id}/${c.episode.item_id}`}
        className={styles.upNextLink}
        aria-label={`${c.series_title} · ${episodeLine(c)}`}
      >
        <span
          className={styles.still}
          style={{
            background: heroBackground(backdropOf(home, c.series_id), seriesTone(c.series_title)),
          }}
        >
          <Progress value={progress(c.episode.runtime_seconds, c.position_seconds, false)} />
        </span>
        <span className={styles.stack}>
          <b className={styles.clip}>{c.series_title}</b>
          <span className={`${styles.muted} ${styles.clip}`}>
            {[episodeLine(c), leftLine(c), c.view_name].filter(Boolean).join(' · ')}
          </span>
        </span>
      </Link>
      {/* Beside the card's link rather than in it: playing skips the episode page. */}
      <Link
        to={`/play/${c.episode.item_id}`}
        className={styles.play}
        aria-label={`${c.position_seconds > 0 ? 'Resume' : 'Play'} ${c.series_title} ${episodeLine(c)}`}
        title={c.position_seconds > 0 ? 'Resume' : 'Play'}
      >
        <PlayIcon />
      </Link>
    </div>
  )
}

const WEEK_DAYS = 8
const DAY_MS = 24 * 3600 * 1000
const weekdayShort = new Intl.DateTimeFormat(LOCALE, { weekday: 'short' })

/** Yesterday, today and the six days after, each with what airs that day. */
function weekDays(home: Home, now: Date) {
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1)
  const days = Array.from({ length: WEEK_DAYS }, (_, i) => {
    const date = new Date(start.getTime() + i * DAY_MS + 12 * 3600 * 1000)
    const day = new Date(date.getFullYear(), date.getMonth(), date.getDate())
    return { date: day, today: i === 1, episodes: [] as ComingUp[] }
  })
  const byDay = new Map(days.map((d) => [d.date.toDateString(), d]))
  for (const u of [...home.recently_aired, ...home.coming_up]) {
    const date = airDay(u.episode.airs_at ?? 0, u.episode.airs_time_known)
    byDay.get(date.toDateString())?.episodes.push(u)
  }
  return days
}

/** What aired yesterday and airs over the coming week, a column per day. */
function ThisWeek({ home }: { home: Home }) {
  const [now] = useState(() => new Date())
  const days = weekDays(home, now)
  return (
    <section aria-labelledby="home-week" className={styles.section}>
      <div className={styles.sectionHead}>
        <h2 id="home-week">This week</h2>
        <Link to="/calendar" className={styles.seeAll}>
          Calendar
        </Link>
      </div>
      <ol className={styles.week}>
        {days.map((d) => (
          <li
            key={d.date.toDateString()}
            className={[
              styles.weekDay,
              d.today ? styles.today : '',
              d.episodes.length > 0 ? styles.airing : '',
            ].join(' ')}
            aria-current={d.today ? 'date' : undefined}
          >
            <span className={styles.dayLabel}>
              {weekdayShort.format(d.date)} <b>{d.date.getDate()}</b>
            </span>
            {d.episodes.map((u) => (
              <Link
                key={u.episode.item_id}
                to={`/tv/${u.series_id}/${u.episode.item_id}`}
                className={styles.weekEpisode}
              >
                <b>{u.series_title}</b>
                <span>
                  {episodeLabel(u.episode.number)} · {u.episode.title}
                </span>
              </Link>
            ))}
          </li>
        ))}
      </ol>
    </section>
  )
}

interface Added {
  item: ItemRef
  at: number
  episodes: number
}

/** The latest addition per series or movie, with the episodes added since. */
function latestAdditions(entries: ActivityEntry[]): Added[] {
  const out = new Map<string, Added>()
  for (const entry of entries) {
    const item = entry.item
    if (!item) {
      continue
    }
    const key = `${item.kind}:${item.id}`
    const items = (entry.payload as { items?: unknown }).items
    const count = Array.isArray(items) ? items.length : 0
    const seen = out.get(key)
    if (seen) {
      seen.episodes += count
    } else {
      out.set(key, { item, at: entry.at, episodes: count })
    }
  }
  return [...out.values()].slice(0, ADDED_LIMIT)
}

function RecentlyAdded({ home }: { home: Home }) {
  const added = useApi('home-added', () => api.activity({ kinds: 'items_added', limit: 30 }))
  useLiveEvents(['activity.added'], added.reload, 1000)
  const additions = latestAdditions(added.data?.entries ?? [])
  if (additions.length === 0) {
    return null
  }
  return (
    <section aria-labelledby="home-added" className={styles.section}>
      <div className={styles.sectionHead}>
        <h2 id="home-added">Recently added</h2>
        <Link to="/activity" className={styles.seeAll}>
          Activity
        </Link>
      </div>
      <div className={`${styles.rail} ${styles.addedRail}`}>
        {additions.map(({ item, at, episodes }) => {
          const movie = item.kind === 'movie'
          const summary = movie
            ? home.movies.find((m) => m.id === item.id)
            : home.shows.find((s) => s.id === item.id)
          const poster = summary ? artworkUrl(summary, 'poster') : null
          return (
            <Link
              key={`${item.kind}:${item.id}`}
              to={itemPath(movie ? 'movie' : 'series', item.id, item.library_id)}
              className={styles.added}
            >
              <span className={styles.addedArt} style={{ background: seriesTone(item.title) }}>
                <CoverImage src={poster} alt={item.title} />
                {!movie && episodes > 0 && (
                  <span className={styles.badge}>
                    {episodes} {episodes === 1 ? 'ep' : 'eps'}
                  </span>
                )}
                {!poster && <span className={styles.addedTitle}>{item.title}</span>}
              </span>
              <span className={styles.muted}>Added {ago(at)}</span>
            </Link>
          )
        })}
      </div>
    </section>
  )
}

const FEED_LENGTH = 5

/** What other people watched. Hidden while there's nothing. */
function FriendsWatching() {
  const me = useUser()
  const watching = useApi('home-watching', () =>
    api.activity({ kinds: 'watched,stopped_watching', limit: 20 }),
  )
  useLiveEvents(['activity.added'], watching.reload, 1000)
  const friends = (watching.data?.entries ?? [])
    .filter((e) => e.actor?.id !== me.id)
    .slice(0, FEED_LENGTH)
  if (friends.length === 0) {
    return null
  }
  return (
    <section aria-labelledby="home-friends" className={`${styles.section} ${styles.friends}`}>
      <h2 id="home-friends">Friends are watching</h2>
      <ActivityList entries={friends} empty="" />
    </section>
  )
}
