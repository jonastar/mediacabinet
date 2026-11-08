import { api, artworkUrl, type Episode, type OrderingNumber } from '../api/client'
import { useApi } from '../api/useApi'
import { CoverImage } from '../components/Artwork'
import { Badge } from '../components/Badge'
import { PlayIcon } from '../components/Icons'
import { Link } from '../components/Link'
import { EpisodeCast } from '../components/People'
import { Progress } from '../components/Progress'
import { Failed, Loading } from '../components/Status'
import {
  airDate,
  bytes,
  episodeLabel,
  episodeTone,
  fileLabel,
  matchSummary,
  runtime,
  seriesTone,
} from '../lib/format'
import { useLiveEvents } from '../lib/live'
import { useMode } from '../lib/prefs'
import { episodePath, folderPath, itemPath, seasonPath, useCanonicalPath } from '../lib/router'
import { NotFoundPage } from './NotFoundPage'
import {
  Breadcrumbs,
  DetailArtwork,
  DetailHeader,
  HealthTiles,
  MetaLine,
} from './detail/DetailHeader'
import detail from './detail/Detail.module.css'
import { facts } from './detail/metadata'
import { Neighbour, NoNeighbour } from './detail/Neighbour'
import { Inspector } from './episode/Inspector'
import { episodeView, seasonName } from './series/episodeView'
import styles from './EpisodePage.module.css'

/** How many episodes "More from Season N" shows, starting around this one. */
const MORE_COUNT = 6

const pad = (n: number) => String(n).padStart(2, '0')

const ORDER_LABELS: Record<string, string> = {
  aired: 'Aired order',
  dvd: 'DVD order',
  absolute: 'Absolute',
}

function orderFact(n: OrderingNumber) {
  return {
    label: ORDER_LABELS[n.kind] ?? `${n.ordering} order`,
    value:
      n.number == null
        ? null
        : n.season != null
          ? `S${pad(n.season)}E${pad(n.number)}`
          : String(n.number),
  }
}

/** `Season 1 · Episode 1`, or the episode's plain number in an ordering without seasons. */
function position(e: Episode) {
  const n = e.number
  if (n?.style === 'season' && n.season != null && n.number != null) {
    const season = n.season === 0 ? 'Specials' : `Season ${n.season}`
    return `${season} · Episode ${n.number}`
  }
  return n?.number != null ? `Episode ${n.number}` : 'Episode'
}

/** One episode: what it is, playing it, and the ones around it in the series' ordering. */
export function EpisodePage({ seriesId, id }: { seriesId: number; id: number }) {
  const [mode] = useMode()
  const series = useApi(`series:${seriesId}`, () => api.seriesDetail(seriesId))
  // In the ordering the user picked for the series, so previous and next match its page.
  const episodes = useApi(`episodes:${seriesId}:`, () => api.episodes(seriesId))
  const about = useApi(`about:${id}`, () => api.about(id))
  const seriesAbout = useApi(`about:${seriesId}`, () => api.about(seriesId))
  const item = useApi(`item:${id}`, () => api.item(id))
  const libraries = useApi('libraries', api.libraries)
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
  const manage = mode === 'manage'
  const v = episodeView(e, index, list.up_next, mode)
  const previous = list.entries[index - 1]
  const next = list.entries[index + 1]
  const link = (other: Episode) => episodePath(seriesId, other.item_id, show.library_id)
  const played = e.watch?.played ?? false
  const resuming = (e.watch?.position_seconds ?? 0) > 0 && !played
  const available = e.availability === 'available'
  const presentFiles = e.files.filter((f) => f.status === 'present')
  const setPlayed = async () => {
    await api.setPlayed(e.item_id, !played)
    await episodes.reload()
  }
  const still = artworkUrl({ id: e.item_id, artwork: e.artwork }, 'still')
  const aired = e.airs_at != null ? airDate(e.airs_at, e.airs_time_known) : null
  const minutes = runtime(e.runtime_seconds)

  // The season's episodes around this one, or the ordering's when it has no seasons.
  const season = e.number?.style === 'season' ? e.number.season : null
  const siblings = list.entries.filter((other) => season == null || other.number?.season === season)
  const here = siblings.findIndex((other) => other.item_id === id)
  const windowStart = Math.max(0, Math.min(here - 1, siblings.length - MORE_COUNT))
  const more = siblings.slice(windowStart, windowStart + MORE_COUNT)
  const watched = siblings.filter(
    (other) => other.watch?.played && other.availability !== 'upcoming',
  ).length
  const airedCount = siblings.filter((other) => other.availability !== 'upcoming').length
  const moreTitle =
    season == null ? 'More episodes' : season === 0 ? 'More specials' : `More from Season ${season}`

  const library = libraries.data?.find((l) => l.id === show.library_id)
  const seriesLink = itemPath('series', seriesId, show.library_id)
  const seasonLink = season == null ? seriesLink : seasonPath(seriesLink, season)
  const crumbs = [
    library
      ? { to: folderPath(library.id, ''), label: library.name }
      : { to: '/tv', label: 'TV Shows' },
    { to: seriesLink, label: show.title },
    ...(season == null ? [] : [{ to: seasonLink, label: seasonName(season) }]),
  ]

  const viewerFacts = facts(
    { label: 'Aired', value: aired },
    { label: 'Runtime', value: minutes },
    ...(item.data?.numbers.map(orderFact) ?? []),
    {
      label: 'File',
      value:
        presentFiles.length > 1
          ? `${presentFiles.length} versions`
          : presentFiles[0] && fileLabel(presentFiles[0]),
    },
  )
  const editorFacts = facts(
    { label: 'Status', value: v.badge.label },
    { label: 'Monitored', value: e.monitored ? 'Yes' : 'No' },
    { label: 'Matched by', value: matchSummary(e.files) },
    { label: 'File', value: presentFiles[0] && fileLabel(presentFiles[0]) },
  )

  return (
    <DetailHeader
      titleId="episode-title"
      backdrop={artworkUrl(show, 'backdrop')}
      tone={seriesTone(show.title)}
      wide
      back={<Breadcrumbs crumbs={crumbs} />}
      artwork={
        <DetailArtwork src={still} tone={v.tone} wide>
          {available && (
            <Link
              to={`/play/${e.item_id}`}
              className={detail.artPlay}
              aria-label={`Play ${v.label}`}
            >
              <PlayIcon />
            </Link>
          )}
          {v.progress > 0 && (
            <span className={detail.artProgress}>
              <Progress value={v.progress} />
            </span>
          )}
        </DetailArtwork>
      }
      facts={manage ? editorFacts : viewerFacts}
      below={
        <>
          {manage && <Inspector itemId={e.item_id} files={e.files} onChanged={episodes.reload} />}
          {!manage && more.length > 1 && (
            <section aria-labelledby="more-title" className={detail.section}>
              <div className={detail.sectionHead}>
                <h2 id="more-title">{moreTitle}</h2>
                <span className={detail.sectionNote}>
                  {watched} of {airedCount} watched
                </span>
              </div>
              <div className={styles.more}>
                {more.map((other) => {
                  const current = other.item_id === id
                  const upNext = list.entries[list.up_next ?? -1]?.item_id === other.item_id
                  const otherStill = artworkUrl(
                    { id: other.item_id, artwork: other.artwork },
                    'still',
                  )
                  return (
                    <Link
                      key={other.item_id}
                      to={link(other)}
                      className={styles.moreCard}
                      aria-current={current ? 'page' : undefined}
                    >
                      <span
                        className={`${styles.moreStill} ${current ? styles.moreCurrent : ''}`}
                        style={{ background: episodeTone(other.item_id) }}
                      >
                        <CoverImage src={otherStill} />
                      </span>
                      <span className={styles.moreCode}>
                        {episodeLabel(other.number)}
                        {current && ' · Now viewing'}
                        {!current && upNext && ' · Up next'}
                      </span>
                      <span className={styles.moreTitle}>{other.title}</span>
                    </Link>
                  )
                })}
              </div>
            </section>
          )}
          {!manage && <EpisodeCast about={about.data} cast={seriesAbout.data?.cast ?? []} />}
        </>
      }
    >
      <div className={detail.eyebrow}>
        <Link to={seasonLink} className={detail.eyebrowLink}>
          {position(e)}
        </Link>
        <Badge badge={v.badge} small />
      </div>
      <h1 id="episode-title" className={detail.title}>
        {e.title}
      </h1>
      <MetaLine parts={[aired, minutes]} about={about.data} />
      {e.overview && <p className={detail.overview}>{e.overview}</p>}
      {manage && <p className={styles.tech}>{v.tech}</p>}
      <div className={detail.actions}>
        {available && (
          <Link to={`/play/${e.item_id}`} className={detail.primary}>
            <PlayIcon />
            {resuming ? 'Resume' : played ? 'Play again' : 'Play'}
          </Link>
        )}
        {available && resuming && (
          <Link to={`/play/${e.item_id}?start=0`} className={detail.secondary}>
            Play from start
          </Link>
        )}
        {!manage && (
          <button type="button" className={detail.secondary} onClick={setPlayed}>
            {played ? 'Mark unwatched' : 'Mark watched'}
          </button>
        )}
      </div>

      {manage && (
        <HealthTiles
          title="Files"
          tiles={[
            { label: 'Linked', value: e.files.length },
            {
              label: 'Missing',
              value: e.files.length - presentFiles.length,
              warn: e.files.length > presentFiles.length,
            },
            {
              label: 'On disk',
              value:
                presentFiles.length > 0
                  ? bytes(presentFiles.reduce((sum, f) => sum + f.size, 0))
                  : '—',
            },
          ]}
        />
      )}
      {(previous || next) && (
        <nav aria-label="Previous and next episode" className={detail.featured}>
          <div className={styles.neighbours}>
            {previous ? <Neighbour episode={previous} to={link(previous)} /> : <NoNeighbour />}
            {next ? (
              <Neighbour episode={next} to={link(next)} next highlight />
            ) : (
              <NoNeighbour next />
            )}
          </div>
        </nav>
      )}
    </DetailHeader>
  )
}
