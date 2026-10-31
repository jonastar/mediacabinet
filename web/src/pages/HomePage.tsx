import { useState } from 'react'
import {
  api,
  artworkUrl,
  type ActivityEntry,
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
import { ago, airDayTile, episodeLabel, minutesLeft, progress, seriesTone } from '../lib/format'
import { useMode } from '../lib/prefs'
import { Link } from '../components/Link'
import { ActivityList } from '../components/ActivityFeed'
import { useUser } from '../lib/session'
import styles from './HomePage.module.css'

const RAIL_LIMIT = 12
const UP_NEXT_LIMIT = 3
const ADDED_LIMIT = 6

const headerDate = new Intl.DateTimeFormat(undefined, {
  weekday: 'long',
  day: 'numeric',
  month: 'long',
})

function greeting(hour: number) {
  if (hour < 5) {
    return 'Good night'
  }
  if (hour < 12) {
    return 'Good morning'
  }
  return hour < 18 ? 'Good afternoon' : 'Good evening'
}

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
      {mode === 'manage' && <NeedsAttention home={data} />}
      {(data.keep_watching.length > 0 || data.coming_up.length > 0) && (
        <div className={styles.columns}>
          <UpNext home={data} />
          <ThisWeek home={data} />
        </div>
      )}
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
      <div className={styles.greeting}>
        <span className={styles.eyebrow}>{headerDate.format(now)}</span>
        <h1 className={styles.title}>{greeting(now.getHours())}</h1>
      </div>
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

function NeedsAttention({ home }: { home: Home }) {
  const a = home.attention
  const cards = [
    {
      to: '/review',
      value: a.review,
      color: 'var(--state-attention)',
      label: 'Matches to review',
      note: 'Below the confidence threshold',
    },
    {
      to: '/libraries',
      value: a.missing_files,
      color: 'var(--state-missing)',
      label: 'Files missing',
      note: a.offline_roots.length
        ? `${a.offline_roots.join(', ')} offline · nothing removed`
        : 'Not found on the last scan',
    },
    {
      to: '/wanted',
      value: a.wanted,
      label: 'Wanted',
      note: a.in_progress
        ? `${a.in_progress} in progress via ${a.status_sources.join(', ')}`
        : 'Monitored and not in the library',
    },
    { to: '/tv', value: a.gaps, label: 'Gaps', note: `In ${a.gap_series} series` },
  ]
  return (
    <section aria-labelledby="home-attention" className={styles.section}>
      <h2 id="home-attention">Needs attention</h2>
      <div className={styles.attention}>
        {cards.map((c) => (
          <Link key={c.label} to={c.to} className={styles.attentionCard}>
            <span className={styles.attentionValue} style={{ color: c.color }}>
              {c.value}
            </span>
            <span className={styles.attentionLabel}>{c.label}</span>
            <span className={styles.attentionNote}>{c.note}</span>
          </Link>
        ))}
      </div>
    </section>
  )
}

function backdropOf(home: Home, seriesId: number) {
  const show = home.shows.find((s) => s.id === seriesId)
  return show ? artworkUrl(show, 'backdrop') : null
}

/** The episode to carry on with as a large card, then the next few as rows. */
function UpNext({ home }: { home: Home }) {
  const [first, ...rest] = home.keep_watching
  return (
    <section aria-labelledby="home-up-next" className={styles.section}>
      <div className={styles.sectionHead}>
        <h2 id="home-up-next">Up next</h2>
      </div>
      {first ? (
        <>
          <UpNextCard item={first} home={home} />
          {rest.slice(0, UP_NEXT_LIMIT).map((c) => (
            <UpNextRow key={c.series_id} item={c} home={home} />
          ))}
        </>
      ) : (
        <p className={styles.muted}>Nothing in progress. Pick something from your shows below.</p>
      )}
    </section>
  )
}

function UpNextCard({ item: c, home }: { item: ContinueItem; home: Home }) {
  const left = minutesLeft(c.episode.runtime_seconds, c.position_seconds)
  return (
    <div className={styles.card}>
      <Link
        to={`/tv/${c.series_id}/${c.episode.item_id}`}
        className={styles.cardArt}
        style={{
          background: heroBackground(backdropOf(home, c.series_id), seriesTone(c.series_title)),
        }}
        aria-label={`${c.series_title} · ${episodeLine(c)}`}
      >
        <span />
        <Progress value={progress(c.episode.runtime_seconds, c.position_seconds, false)} />
      </Link>
      <div className={styles.cardBody}>
        <h3 className={styles.cardTitle}>{c.series_title}</h3>
        <p className={styles.muted}>
          {[episodeLine(c), left ? `${left} min left` : null, c.view_name]
            .filter(Boolean)
            .join(' · ')}
        </p>
        <div className={styles.actions}>
          <Link to={`/play/${c.episode.item_id}`} className={styles.primary}>
            {c.position_seconds > 0 ? 'Resume' : 'Play'}
          </Link>
          <Link to={`/tv/${c.series_id}`} className={styles.secondary}>
            Episodes
          </Link>
        </div>
      </div>
    </div>
  )
}

function UpNextRow({ item: c, home }: { item: ContinueItem; home: Home }) {
  return (
    <div className={styles.row}>
      <Link to={`/tv/${c.series_id}/${c.episode.item_id}`} className={styles.rowLink}>
        <span
          className={styles.thumb}
          style={{
            background: heroBackground(backdropOf(home, c.series_id), seriesTone(c.series_title)),
          }}
        />
        <span className={styles.stack}>
          <b>{c.series_title}</b>
          <span className={styles.muted}>
            {[episodeLine(c), leftLine(c), c.view_name].filter(Boolean).join(' · ')}
          </span>
        </span>
      </Link>
      {/* Beside the row's link rather than in it: playing skips the episode page. */}
      <Link
        to={`/play/${c.episode.item_id}`}
        className={styles.play}
        aria-label={`Play ${c.series_title} ${episodeLine(c)}`}
        title="Play"
      >
        <PlayIcon />
      </Link>
    </div>
  )
}

function ThisWeek({ home }: { home: Home }) {
  return (
    <section aria-labelledby="home-week" className={styles.section}>
      <div className={styles.sectionHead}>
        <h2 id="home-week">This week</h2>
        <Link to="/calendar" className={styles.seeAll}>
          Calendar
        </Link>
      </div>
      {home.coming_up.length === 0 ? (
        <p className={styles.muted}>Nothing airing in the next seven days.</p>
      ) : (
        <ul className={styles.week}>
          {home.coming_up.map((u) => {
            const day = airDayTile(u.episode.airs_at ?? 0, u.episode.airs_time_known)
            return (
              <li key={u.episode.item_id}>
                <Link to={`/tv/${u.series_id}/${u.episode.item_id}`} className={styles.weekRow}>
                  <span className={`${styles.day} ${day.today ? styles.today : ''}`}>
                    <small>{day.label}</small>
                    <b>{day.day}</b>
                  </span>
                  <span className={styles.stack}>
                    <b>{u.series_title}</b>
                    <span className={styles.muted}>
                      {episodeLabel(u.episode.number)} · {u.episode.title}
                    </span>
                  </span>
                </Link>
              </li>
            )
          })}
        </ul>
      )}
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
      <div className={`${styles.rail} ${styles.posterRail}`}>
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
              </span>
              <span className={styles.stack}>
                <b>{item.title}</b>
                <span className={styles.muted}>Added {ago(at)}</span>
              </span>
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
