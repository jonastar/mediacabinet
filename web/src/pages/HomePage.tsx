import { useState } from 'react'
import { api, artworkUrl, type ContinueItem, type Home } from '../api/client'
import { heroBackground } from '../lib/artwork'
import { useLiveEvents } from '../lib/live'
import { navigate } from '../lib/router'
import { useApi } from '../api/useApi'
import { PlayIcon } from '../components/Icons'
import { MoviePoster, Poster } from '../components/Poster'
import { Progress } from '../components/Progress'
import { Failed, Loading } from '../components/Status'
import {
  airShortDay,
  airWeekday,
  episodeLabel,
  minutesLeft,
  progress,
  seriesTone,
  time,
} from '../lib/format'
import { useMode } from '../lib/prefs'
import { Link } from '../components/Link'
import styles from './HomePage.module.css'

const RAIL_LIMIT = 12

function continueLine(c: ContinueItem) {
  const left = minutesLeft(c.episode.runtime_seconds, c.position_seconds)
  return {
    episode: `${episodeLabel(c.episode.number)} · ${c.episode.title}`,
    left: left ? `${left} min left` : 'Up next',
  }
}

function heroLine(c: ContinueItem) {
  const left = minutesLeft(c.episode.runtime_seconds, c.position_seconds)
  return [continueLine(c).episode, left ? `${left} min left` : null].filter(Boolean).join(' · ')
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
    <>
      <Hero home={data} />
      <div className={styles.content}>
        {mode === 'manage' && <NeedsAttention home={data} />}
        <KeepWatching items={data.keep_watching} home={data} />
        <ComingUp home={data} />
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
    </>
  )
}

function Hero({ home }: { home: Home }) {
  const hero = home.hero
  const first = home.shows[0]
  const [query, setQuery] = useState('')
  const tone = seriesTone(hero?.series_title ?? first?.title ?? '')
  const show = home.shows.find((s) => s.id === (hero?.series_id ?? first?.id))
  const background = heroBackground(show ? artworkUrl(show, 'backdrop') : null, tone)
  return (
    <section aria-labelledby="home-hero" className={styles.hero} style={{ background }}>
      <form
        role="search"
        className={styles.heroBar}
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
      <div className={styles.heroBody}>
        {hero ? (
          <>
            <span className={styles.eyebrow}>
              Continue{hero.view_name ? ` · ${hero.view_name}` : ''}
            </span>
            <h1 id="home-hero" className={styles.title}>
              {hero.series_title}
            </h1>
            <p className={styles.heroMeta}>{heroLine(hero)}</p>
            <div className={styles.actions}>
              <Link to={`/play/${hero.episode.item_id}`} className={styles.primary}>
                <PlayIcon />
                {hero.position_seconds > 0 ? 'Resume' : 'Play'}
              </Link>
              <Link to={`/tv/${hero.series_id}`} className={styles.secondary}>
                Episodes
              </Link>
            </div>
          </>
        ) : (
          <>
            <span className={styles.eyebrow}>Your library</span>
            <h1 id="home-hero" className={styles.title}>
              {first ? first.title : 'Media Cabinet'}
            </h1>
            {first && (
              <div className={styles.actions}>
                <Link to={`/tv/${first.id}`} className={styles.primary}>
                  Episodes
                </Link>
              </div>
            )}
          </>
        )}
      </div>
    </section>
  )
}

function NeedsAttention({ home }: { home: Home }) {
  const a = home.attention
  const cards = [
    {
      to: '/review',
      value: a.review,
      color: 'var(--accent)',
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
    <section aria-labelledby="home-attention" className={`${styles.section} ${styles.padded}`}>
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

function KeepWatching({ items, home }: { items: ContinueItem[]; home: Home }) {
  if (items.length === 0) {
    return null
  }
  return (
    <section aria-labelledby="home-keep" className={styles.section}>
      <h2 id="home-keep" className={styles.padded}>
        Keep watching
      </h2>
      <div className={styles.rail}>
        {items.map((c) => {
          const line = continueLine(c)
          return (
            <div key={c.series_id} className={styles.continue}>
              <Link to={`/tv/${c.series_id}/${c.episode.item_id}`} className={styles.continueLink}>
                <div
                  className={styles.continueArt}
                  style={{
                    background: heroBackground(
                      backdropOf(home, c.series_id),
                      seriesTone(c.series_title),
                    ),
                  }}
                >
                  <span>{c.series_title}</span>
                  <Progress
                    value={progress(c.episode.runtime_seconds, c.position_seconds, false)}
                  />
                </div>
                <span className={styles.stack}>
                  <b>{line.episode}</b>
                  <span className={styles.muted}>
                    {[c.view_name ? `${c.view_name}` : null, line.left].filter(Boolean).join(' · ')}
                  </span>
                </span>
              </Link>
              {/* Beside the card's link rather than in it: playing skips the episode page. */}
              <Link
                to={`/play/${c.episode.item_id}`}
                className={styles.continuePlay}
                aria-label={`Play ${line.episode}`}
                title="Play"
              >
                <PlayIcon />
              </Link>
            </div>
          )
        })}
      </div>
    </section>
  )
}

function ComingUp({ home }: { home: Home }) {
  if (home.coming_up.length === 0) {
    return null
  }
  return (
    <section aria-labelledby="home-up" className={styles.section}>
      <h2 id="home-up" className={styles.padded}>
        Coming this week
      </h2>
      <div className={styles.rail}>
        {home.coming_up.map((u) => (
          <Link
            key={u.episode.item_id}
            to={`/tv/${u.series_id}/${u.episode.item_id}`}
            className={styles.upcoming}
          >
            <div className={styles.dayTile} style={{ background: seriesTone(u.series_title) }}>
              <b>{airWeekday(u.episode.airs_at ?? 0, u.episode.airs_time_known)}</b>
              <span>
                {u.episode.airs_time_known
                  ? time(u.episode.airs_at ?? 0)
                  : airShortDay(u.episode.airs_at ?? 0, false).replace(/^\D+/, '')}
              </span>
            </div>
            <span className={styles.stack}>
              <b>{u.series_title}</b>
              <span className={styles.muted}>
                {episodeLabel(u.episode.number)} · {u.episode.title}
              </span>
            </span>
          </Link>
        ))}
      </div>
    </section>
  )
}
