import { useEffect, useState } from 'react'
import { api, artworkUrl, type Episode, type Playlist, type View } from '../api/client'
import { useApi } from '../api/useApi'
import { CoverImage } from '../components/Artwork'
import { Countdown } from '../components/Countdown'
import { EditDetails } from '../components/EditDetails'
import { AirDateIcon, PencilIcon, PlayIcon, PlusIcon } from '../components/Icons'
import { LibraryBack } from '../components/LibraryBack'
import { Link } from '../components/Link'
import { CastSection } from '../components/People'
import { Progress } from '../components/Progress'
import { Segmented } from '../components/Segmented'
import { Failed, Loading } from '../components/Status'
import {
  airDate,
  airDayLabel,
  bytes,
  episodeLabel,
  episodeTone,
  minutesLeft,
  progress,
  runtime,
  seriesTone,
} from '../lib/format'
import { useLiveEvents } from '../lib/live'
import { useMode } from '../lib/prefs'
import { episodePath, itemPath, useCanonicalPath, useQueryParam } from '../lib/router'
import { useUser } from '../lib/session'
import { DetailArtwork, DetailHeader, HealthTiles, MetaLine } from './detail/DetailHeader'
import detail from './detail/Detail.module.css'
import { LOCKABLE, facts, lockedLabel, matchedTo, seriesStatus } from './detail/metadata'
import { Neighbour } from './detail/Neighbour'
import { SearchPicker } from './review/Pickers'
import { EpisodeTable } from './series/EpisodeTable'
import { Episodes } from './series/Episodes'
import { ManagePanels } from './series/ManagePanels'
import { PlaylistEditor } from './series/PlaylistEditor'
import styles from './SeriesPage.module.css'

/** What the current ordering means; nothing when it's the only one. */
function viewNote(view: View | undefined, views: View[]): string | null {
  if (view?.kind === 'playlist') {
    return `${view.name} playlist. Up next follows it.`
  }
  if (!view || views.length <= 1) {
    return null
  }
  const base: Record<string, string> = {
    aired: 'Aired order. Seasons as broadcast.',
    dvd: 'DVD order. Seasons as released on disc.',
    absolute: 'Absolute order. One continuous numbering.',
    story: 'Story order. Episodes in the order events happen.',
    custom: `${view.name}: a custom ordering.`,
  }
  const note = base[view.kind] ?? view.name
  return view.is_default ? note.replace('.', ' (default).') : note
}

/** Each file once: an episode in two orderings, or a file shared by episodes, counts once. */
function onDisk(entries: Episode[]) {
  const sizes = new Map<number, number>()
  for (const e of entries) {
    for (const f of e.files) {
      if (f.status === 'present') {
        sizes.set(f.file_id, f.size)
      }
    }
  }
  return [...sizes.values()].reduce((sum, size) => sum + size, 0)
}

function ContinueCard({ episode: e }: { episode: Episode }) {
  const position = e.watch?.position_seconds ?? 0
  const started = position > 0
  const left = minutesLeft(e.runtime_seconds, position)
  const still = artworkUrl({ id: e.item_id, artwork: e.artwork }, 'still')
  return (
    <Link to={`/play/${e.item_id}`} className={detail.episodeCard}>
      <span className={detail.cardStill} style={{ background: episodeTone(e.item_id) }}>
        <CoverImage src={still} />
        <span className={detail.cardPlay}>
          <PlayIcon />
        </span>
        {started && (
          <span className={detail.artProgress}>
            <Progress value={progress(e.runtime_seconds, position, false)} />
          </span>
        )}
      </span>
      <span className={detail.cardText}>
        <span className={detail.cardEyebrow}>Up next · {episodeLabel(e.number)}</span>
        <span className={detail.cardTitle}>{e.title}</span>
        <span className={detail.cardMeta}>
          {[runtime(e.runtime_seconds), left ? `${left} min left` : null]
            .filter(Boolean)
            .join(' · ')}
        </span>
      </span>
    </Link>
  )
}

function NextEpisodeCard({
  episode: e,
  network,
}: {
  episode: { airs_at: number; airs_time_known: boolean; title: string; number?: Episode['number'] }
  network: string | undefined
}) {
  return (
    <div className={styles.nextCard}>
      <span className={styles.nextIcon}>
        <AirDateIcon />
      </span>
      <span className={styles.nextText}>
        <span className={styles.nextEyebrow}>Next episode · {episodeLabel(e.number)}</span>
        <span className={styles.nextCountdown}>
          <Countdown airsAt={e.airs_at} timeKnown={e.airs_time_known} />
        </span>
        <span className={styles.nextMeta}>
          {[e.title, airDayLabel(e.airs_at, e.airs_time_known), network]
            .filter(Boolean)
            .join(' · ')}
        </span>
      </span>
    </div>
  )
}

export function SeriesPage({ id }: { id: number }) {
  const [mode] = useMode()
  const series = useApi(`series:${id}`, () => api.seriesDetail(id))
  const about = useApi(`about:${id}`, () => api.about(id))
  const libraries = useApi('libraries', api.libraries)
  useCanonicalPath(series.data ? itemPath('series', id, series.data.library_id) : null)
  // The view the user picked on this page; until then, their saved one.
  const [pickedView, setPickedView] = useState<string | null>(null)
  const [pendingMonitor, setPendingMonitor] = useState(new Map<number, boolean>())
  const [seriesMonitorPending, setSeriesMonitorPending] = useState(false)
  const [editing, setEditing] = useState(false)
  const [fixingMatch, setFixingMatch] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  /** The playlist being created (no playlist) or changed. */
  const [editor, setEditor] = useState<{ playlist?: Playlist; picks?: number[] } | null>(null)
  const user = useUser()

  const view = pickedView ?? series.data?.selected_view ?? null
  const episodes = useApi(series.data ? `episodes:${id}:${view ?? ''}` : null, () =>
    api.episodes(id, view),
  )
  // Files, links, wanted state and statuses change while you look.
  useLiveEvents(
    [
      'file.added',
      'file.missing',
      'file.restored',
      'file.moved',
      'file.removed',
      'link.created',
      'item.wanted',
      'item.unwanted',
      'item.edited',
      'series.refreshed',
      'series.moved',
      'status.changed',
    ],
    () => Promise.all([series.reload(), episodes.reload()]),
  )

  // A link to a season lands on its episodes, once what's above them has its final height.
  const linkedSeason = useQueryParam('season')
  const settled = episodes.data != null && (about.data != null || about.error != null)
  useEffect(() => {
    if (settled && linkedSeason != null) {
      requestAnimationFrame(() =>
        (
          document.getElementById(`season-${linkedSeason}`) ??
          document.getElementById('episodes-title')
        )?.scrollIntoView(),
      )
    }
  }, [settled, linkedSeason])

  if (series.error) {
    return <Failed error={series.error} onRetry={series.reload} />
  }
  if (!series.data) {
    return <Loading />
  }
  const s = series.data
  const list = episodes.data
  const manage = mode === 'manage'

  const pickView = (next: string) => {
    setPickedView(next)
    // Saving the view makes it drive "next up" everywhere, e.g. on Home.
    api.setView(id, next).then(series.reload, () => undefined)
  }

  const toggleEpisodeMonitor = async (e: Episode) => {
    const itemId = e.item_id
    setPendingMonitor((m) => new Map(m).set(itemId, !e.monitored))
    try {
      await api.setMonitored(itemId, !e.monitored)
      await Promise.all([episodes.reload(), series.reload()])
    } finally {
      setPendingMonitor((m) => {
        const next = new Map(m)
        next.delete(itemId)
        return next
      })
    }
  }

  const toggleSeriesMonitor = async () => {
    setSeriesMonitorPending(true)
    try {
      await api.setMonitored(id, !s.monitored)
      await Promise.all([series.reload(), episodes.reload()])
    } finally {
      setSeriesMonitorPending(false)
    }
  }

  const reloadAll = () => Promise.all([series.reload(), episodes.reload(), about.reload()])

  const playlistId = () => Number(currentView?.id.replace('playlist:', ''))

  const editPlaylist = async () => {
    const playlists = await api.playlists(id)
    const playlist = playlists.find((p) => p.id === playlistId())
    if (playlist) {
      setEditor({ playlist })
    }
  }

  const deletePlaylist = async () => {
    if (!currentView || !window.confirm(`Delete the ${currentView.name} playlist?`)) {
      return
    }
    await api.deletePlaylist(playlistId())
    setPickedView(null)
    await reloadAll()
  }

  const setPlayed = async (marked: Episode[], played: boolean) => {
    const ids = marked.map((e) => e.item_id)
    try {
      await (ids.length === 1 ? api.setPlayed(ids[0], played) : api.setPlayed(id, played, ids))
      setNotice(null)
      await episodes.reload()
    } catch (e) {
      setNotice(`Couldn't change watched state: ${(e as Error).message}`)
    }
  }

  const runAction = async (label: string, action: () => Promise<string>) => {
    setNotice(`${label}…`)
    try {
      setNotice(await action())
      await reloadAll()
    } catch (e) {
      setNotice(`${label} failed: ${(e as Error).message}`)
    }
  }

  const tone = seriesTone(s.title)
  const upNext = list?.up_next != null ? list.entries[list.up_next] : null
  // The episodes either side of up next, in the ordering being watched.
  const before = list?.up_next != null ? list.entries[list.up_next - 1] : undefined
  const after = list?.up_next != null ? list.entries[list.up_next + 1] : undefined
  const resuming = (upNext?.watch?.position_seconds ?? 0) > 0
  const first = list?.entries.find((e) => e.availability === 'available') ?? null
  const next = s.next_airing
  const network = about.data?.networks[0]
  const status = seriesStatus(s.series_status)
  const counts = list?.counts ?? s.counts
  const currentView = s.views.find((v) => v.id === (view ?? list?.view))
  const locked = s.locked_fields.length > 0

  const numbered = list?.entries.filter((e) => e.number?.style === 'season') ?? []
  const seasonCount = new Set(
    numbered.map((e) => e.number?.season).filter((n) => n != null && n > 0),
  ).size
  const firstAired = list?.entries
    .filter((e) => e.airs_at != null && e.number?.season !== 0)
    .sort((a, b) => (a.airs_at ?? 0) - (b.airs_at ?? 0))[0]
  const location = s.locations.find((l) => l.is_primary) ?? s.locations[0]

  const viewerFacts = facts(
    { label: 'Network', value: network },
    {
      label: 'First aired',
      value: firstAired && airDate(firstAired.airs_at, firstAired.airs_time_known),
    },
    { label: 'Status', value: status },
    {
      label: 'Next episode',
      value: next?.airs_at != null && airDayLabel(next.airs_at, next.airs_time_known),
    },
    { label: 'Seasons', value: seasonCount > 0 && seasonCount },
    {
      label: 'In library',
      value: `${counts.available} ${counts.available === 1 ? 'episode' : 'episodes'}`,
    },
  )
  const editorFacts = facts(
    { label: 'Matched to', value: matchedTo(s.external_ids) },
    { label: 'Library', value: libraries.data?.find((l) => l.id === s.library_id)?.name },
    {
      label: 'Path',
      value: location && `${location.root_path.replace(/\/$/, '')}/${location.path}`,
    },
    { label: 'Locked fields', value: lockedLabel(s.locked_fields) },
  )

  const order = !manage && s.views.length > 1 && (
    <label className={styles.order}>
      Order
      <select value={view ?? list?.view ?? ''} onChange={(e) => pickView(e.target.value)}>
        {s.views.map((v) => (
          <option key={v.id} value={v.id}>
            {v.name}
          </option>
        ))}
      </select>
    </label>
  )
  const editorOrder = manage && s.views.length > 1 && (
    <Segmented
      label="Episode order"
      variant="box"
      value={view ?? list?.view}
      onChange={pickView}
      options={s.views.map((v) => ({ value: v.id, label: v.name }))}
    />
  )
  const note = (
    <p className={styles.viewNote}>
      {viewNote(currentView, s.views)}
      {currentView?.kind === 'playlist' && (!currentView.shared || user.is_admin) && (
        <>
          {' '}
          <button type="button" className={styles.noteAction} onClick={editPlaylist}>
            Change
          </button>
          <button type="button" className={styles.noteAction} onClick={deletePlaylist}>
            Delete
          </button>
        </>
      )}
      {!editor && (
        <button type="button" className={styles.noteAction} onClick={() => setEditor({})}>
          New playlist
        </button>
      )}
    </p>
  )
  const episodesStatus = episodes.error ? (
    <Failed error={episodes.error} onRetry={episodes.reload} />
  ) : !list ? (
    <Loading label="Loading episodes…" />
  ) : undefined
  const startPlaylist = (picked: Episode[]) => setEditor({ picks: picked.map((e) => e.item_id) })
  const episodeLink = (e: Episode) => episodePath(id, e.item_id, s.library_id)

  return (
    <DetailHeader
      titleId="series-title"
      backdrop={artworkUrl(s, 'backdrop')}
      tone={tone}
      back={
        <LibraryBack
          libraryId={s.library_id}
          fallback={{ to: '/tv', label: 'TV Shows' }}
          className={detail.back}
        />
      }
      barActions={
        manage && (
          <>
            <button
              type="button"
              className={detail.barButton}
              disabled={s.locations.length === 0}
              onClick={() =>
                runAction('Rescan', async () => {
                  await api.rescanSeries(id)
                  return 'Rescan started; files appear as the scan finishes.'
                })
              }
            >
              Rescan files
            </button>
            <button
              type="button"
              className={detail.barButton}
              onClick={() =>
                runAction('Refresh', async () => {
                  const r = await api.refreshItem(id)
                  return `Refreshed: ${r.episodes_added} new, ${r.episodes_updated} updated episodes.`
                })
              }
            >
              Refresh metadata
            </button>
            <button
              type="button"
              className={detail.barButton}
              aria-expanded={fixingMatch}
              onClick={() => setFixingMatch(!fixingMatch)}
            >
              Fix match
            </button>
          </>
        )
      }
      artwork={
        <DetailArtwork src={artworkUrl(s, 'poster')} tone={tone} label={s.title} editing={manage}>
          {manage && (
            <button
              type="button"
              className={detail.changeArt}
              disabled
              title="Changing artwork isn't built yet"
            >
              Change artwork
            </button>
          )}
        </DetailArtwork>
      }
      facts={manage ? editorFacts : viewerFacts}
      below={
        <>
          {manage && editing && (
            <EditDetails
              itemId={id}
              title={s.title}
              year={s.year}
              overview={s.overview}
              onSaved={() => {
                setEditing(false)
                reloadAll()
              }}
              onCancel={() => setEditing(false)}
            />
          )}
          {manage && fixingMatch && (
            <section aria-label="Fix match" className={detail.section}>
              <SearchPicker
                kind="tv"
                initial={s.title}
                year={s.year}
                onPick={(r) =>
                  runAction('Fix match', async () => {
                    await api.rematchSeries(id, r.id)
                    setFixingMatch(false)
                    return 'Match changed.'
                  })
                }
              />
            </section>
          )}
          {manage && (
            <ManagePanels
              series={s}
              monitorPending={seriesMonitorPending}
              onToggleMonitor={toggleSeriesMonitor}
              onChanged={reloadAll}
              onNewPlaylist={() => setEditor({})}
            />
          )}
          {editor && (
            <PlaylistEditor
              key={editor.playlist?.id ?? 'new'}
              series={s}
              playlist={editor.playlist}
              picks={editor.picks}
              canShare={user.is_admin}
              onSaved={(p) => {
                setEditor(null)
                pickView(`playlist:${p.id}`)
                episodes.reload()
              }}
              onCancel={() => setEditor(null)}
            />
          )}
          {manage ? (
            <EpisodeTable
              entries={list?.entries ?? []}
              upNext={list?.up_next}
              episodeLink={episodeLink}
              pendingMonitor={pendingMonitor}
              onToggleMonitor={toggleEpisodeMonitor}
              onSetPlayed={setPlayed}
              onAddToPlaylist={startPlaylist}
              order={editorOrder}
              initialSeason={linkedSeason}
              note={note}
              status={episodesStatus}
            />
          ) : (
            <Episodes
              entries={list?.entries ?? []}
              upNext={list?.up_next}
              episodeLink={episodeLink}
              onSetPlayed={setPlayed}
              onAddToPlaylist={startPlaylist}
              order={order}
              initialSeason={linkedSeason}
              note={note}
              status={episodesStatus}
            />
          )}
          {!manage && <CastSection about={about.data} />}
        </>
      }
    >
      <div className={detail.titleRow}>
        <h1 id="series-title" className={detail.title}>
          {s.title}
        </h1>
        {manage && (
          <button
            type="button"
            className={`${detail.round} ${detail.small}`}
            aria-label="Edit title"
            onClick={() => setEditing(true)}
          >
            <PencilIcon />
          </button>
        )}
      </div>
      <MetaLine parts={[s.year, s.genres[0], network, status]} about={about.data} />
      {about.data?.tagline && <p className={detail.tagline}>{about.data.tagline}</p>}
      {s.overview && <p className={detail.overview}>{s.overview}</p>}

      {manage ? (
        <div className={detail.actions}>
          <button type="button" className={detail.primary} onClick={() => setEditing(!editing)}>
            Edit metadata
          </button>
          <button
            type="button"
            className={detail.secondary}
            onClick={() =>
              runAction(locked ? 'Unlock' : 'Lock', async () => {
                await api.setLocks(id, locked ? [] : LOCKABLE)
                return locked ? 'Fields unlocked.' : 'Fields locked from refresh.'
              })
            }
          >
            {locked ? 'Unlock fields' : 'Lock fields'}
          </button>
        </div>
      ) : (
        <div className={detail.actions}>
          {upNext && (
            <Link to={`/play/${upNext.item_id}`} className={detail.primary}>
              <PlayIcon />
              {resuming ? 'Resume' : 'Play'} {episodeLabel(upNext.number)}
            </Link>
          )}
          {first && first !== upNext && (
            <Link to={`/play/${first.item_id}?start=0`} className={detail.secondary}>
              Watch from start
            </Link>
          )}
          <button
            type="button"
            className={detail.round}
            aria-label="Add to collection"
            disabled
            title="Collections aren't built yet"
          >
            <PlusIcon />
          </button>
        </div>
      )}
      {notice && (
        <p className={detail.notice} role="status">
          {notice}
        </p>
      )}

      {manage ? (
        <HealthTiles
          title="Library health"
          tiles={[
            { label: 'Episodes', value: `${counts.available} / ${counts.total}` },
            { label: 'Missing', value: counts.missing, warn: counts.missing > 0 },
            { label: 'Not acquired', value: counts.not_acquired },
            { label: 'Wanted', value: counts.wanted },
            {
              label: 'On disk',
              value: list && currentView?.kind !== 'playlist' ? bytes(onDisk(list.entries)) : '—',
            },
          ]}
        />
      ) : (
        (upNext || next?.airs_at != null) && (
          <section className={detail.featured} aria-label="What's next">
            {upNext && (
              <div className={detail.continueRow}>
                {before && <Neighbour episode={before} to={episodeLink(before)} />}
                <ContinueCard episode={upNext} />
                {after && <Neighbour episode={after} to={episodeLink(after)} next />}
              </div>
            )}
            {next?.airs_at != null && (
              <NextEpisodeCard episode={{ ...next, airs_at: next.airs_at }} network={network} />
            )}
          </section>
        )
      )}
    </DetailHeader>
  )
}
