import { api, artworkUrl, type Episode } from '../api/client'
import { useApi } from '../api/useApi'
import { Badge } from '../components/Badge'
import { BackIcon, PlayIcon } from '../components/Icons'
import { DetailsToggle } from '../components/DetailsToggle'
import { Link } from '../components/Link'
import { Progress } from '../components/Progress'
import { Failed, Loading } from '../components/Status'
import { heroBackground } from '../lib/artwork'
import { episodeLabel, seriesTone } from '../lib/format'
import { useLiveEvents } from '../lib/live'
import { useMode } from '../lib/prefs'
import { episodePath, itemPath, useCanonicalPath } from '../lib/router'
import { NotFoundPage } from './NotFoundPage'
import { Inspector } from './episode/Inspector'
import { episodeView } from './series/episodeView'
import styles from './SeriesPage.module.css'
import own from './EpisodePage.module.css'

function Neighbour({ episode, to, next }: { episode: Episode; to: string; next?: boolean }) {
  return (
    <Link to={to} className={`${own.neighbour} ${next ? own.next : ''}`}>
      <span className={own.neighbourLabel}>{next ? 'Next' : 'Previous'}</span>
      <b>
        {episodeLabel(episode.number)} · {episode.title}
      </b>
    </Link>
  )
}

/** One episode: what it is, playing it, and the ones either side in the series' ordering. */
export function EpisodePage({ seriesId, id }: { seriesId: number; id: number }) {
  const [mode] = useMode()
  const series = useApi(`series:${seriesId}`, () => api.seriesDetail(seriesId))
  // In the ordering the user picked for the series, so previous and next match its page.
  const episodes = useApi(`episodes:${seriesId}:`, () => api.episodes(seriesId))
  useCanonicalPath(series.data ? episodePath(seriesId, id, series.data.library_id) : null)
  useLiveEvents(
    [
      'file.added',
      'file.missing',
      'file.restored',
      'file.removed',
      'link.created',
      'link.changed',
      'link.removed',
      'item.edited',
    ],
    () => episodes.reload(),
  )

  const error = series.error ?? episodes.error
  if (error) {
    return (
      <Failed error={error} onRetry={() => Promise.all([series.reload(), episodes.reload()])} />
    )
  }
  if (!series.data || !episodes.data) {
    return <Loading />
  }
  const list = episodes.data
  const index = list.entries.findIndex((e) => e.item_id === id)
  if (index < 0) {
    return <NotFoundPage />
  }
  const show = series.data
  const e = list.entries[index]
  const v = episodeView(e, index, list.up_next, mode)
  const previous = list.entries[index - 1]
  const next = list.entries[index + 1]
  const link = (other: Episode) => episodePath(seriesId, other.item_id, show.library_id)
  const resuming = (e.watch?.position_seconds ?? 0) > 0 && !e.watch?.played
  const played = e.watch?.played ?? false
  const minutes = e.runtime_seconds ? `${Math.round(e.runtime_seconds / 60)} min` : null
  const meta = [v.meta, minutes].filter(Boolean).join(' · ')
  const setPlayed = async () => {
    await api.setPlayed(e.item_id, !played)
    await episodes.reload()
  }

  return (
    <>
      <section
        aria-labelledby="episode-title"
        className={`${styles.hero} ${own.hero}`}
        style={{ background: heroBackground(artworkUrl(show, 'backdrop'), seriesTone(show.title)) }}
      >
        <div className={styles.heroBar}>
          <Link to={itemPath('series', seriesId, show.library_id)} className={styles.back}>
            <BackIcon />
            {show.title}
          </Link>
          <DetailsToggle />
        </div>
        <div className={styles.heroBody}>
          <span className={own.eyebrow}>
            {v.label}
            <Badge badge={v.badge} small />
          </span>
          <h1 id="episode-title" className={`${styles.title} ${own.title}`}>
            {e.title}
          </h1>
          {meta && <p className={styles.heroMeta}>{meta}</p>}
          {v.progress > 0 && !played && (
            <div className={own.progress}>
              <Progress value={v.progress} />
            </div>
          )}
          {e.overview && <p className={styles.overview}>{e.overview}</p>}
          {mode === 'manage' && <p className={own.tech}>{v.tech}</p>}
          <div className={styles.actions}>
            {e.availability === 'available' && (
              <Link to={`/play/${e.item_id}`} className={styles.primary}>
                <PlayIcon />
                {resuming ? 'Resume' : 'Play'}
              </Link>
            )}
            {e.availability === 'available' && resuming && (
              <Link to={`/play/${e.item_id}?start=0`} className={styles.secondary}>
                Play from start
              </Link>
            )}
            {mode === 'watch' && (
              <button type="button" className={styles.secondary} onClick={setPlayed}>
                {played ? 'Mark unwatched' : 'Mark watched'}
              </button>
            )}
          </div>
        </div>
      </section>

      {mode === 'manage' && (
        <Inspector itemId={e.item_id} files={e.files} onChanged={episodes.reload} />
      )}

      {(previous || next) && (
        <nav aria-label="Other episodes" className={own.neighbours}>
          {previous && <Neighbour episode={previous} to={link(previous)} />}
          {next && <Neighbour episode={next} to={link(next)} next />}
        </nav>
      )}
    </>
  )
}
