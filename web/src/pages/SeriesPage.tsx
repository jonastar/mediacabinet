import { useState } from 'react'
import { api, type Episode, type SeriesDetail, type View } from '../api/client'
import { useApi } from '../api/useApi'
import { BackIcon, GridIcon, ListIcon, PlayIcon } from '../components/Icons'
import { Segmented } from '../components/Segmented'
import { Failed, Loading } from '../components/Status'
import { dayTimeLabel, episodeLabel, seriesTone } from '../lib/format'
import { useEpisodeLayout, useMode } from '../lib/prefs'
import { Link } from '../components/Link'
import { Episodes } from './series/Episodes'
import { ManagePanels } from './series/ManagePanels'
import styles from './SeriesPage.module.css'

const PLAY_NOT_BUILT = "Playback isn't built yet"

function viewNote(view: View | undefined, views: View[]): string {
  if (!view) {
    return 'Every episode, by air date.'
  }
  if (view.kind === 'playlist') {
    return `${view.name} playlist. Up next follows it.`
  }
  const base: Record<string, string> = {
    aired: 'Aired order. Seasons as broadcast.',
    dvd: 'DVD order. Seasons as released on disc.',
    absolute: 'Absolute order. One continuous numbering.',
    story: 'Story order. Episodes in the order events happen.',
    custom: `${view.name}: a custom ordering.`,
  }
  const note = base[view.kind] ?? view.name
  return view.is_default && views.length > 1 ? note.replace('.', ' (default).') : note
}

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

export function SeriesPage({ id }: { id: number }) {
  const [mode] = useMode()
  const [layout, setLayout] = useEpisodeLayout()
  const detail = useApi(`series:${id}`, () => api.seriesDetail(id))
  // The view the user picked on this page; until then, their saved one.
  const [pickedView, setPickedView] = useState<string | null>(null)
  const [pendingMonitor, setPendingMonitor] = useState(new Map<number, boolean>())
  const [seriesMonitorPending, setSeriesMonitorPending] = useState(false)

  const view = pickedView ?? detail.data?.selected_view ?? null
  const episodes = useApi(detail.data ? `episodes:${id}:${view ?? ''}` : null, () =>
    api.episodes(id, view),
  )

  if (detail.error) {
    return <Failed error={detail.error} onRetry={detail.reload} />
  }
  if (!detail.data) {
    return <Loading />
  }
  const series = detail.data
  const list = episodes.data
  const manage = mode === 'manage'

  const pickView = (next: string) => {
    setPickedView(next)
    // Saving the view makes it drive "next up" everywhere, e.g. on Home.
    api.setView(id, next).then(detail.reload, () => undefined)
  }

  const toggleEpisodeMonitor = async (e: Episode) => {
    const itemId = e.item_id
    setPendingMonitor((m) => new Map(m).set(itemId, !e.monitored))
    try {
      await api.setMonitored(itemId, !e.monitored)
      await Promise.all([episodes.reload(), detail.reload()])
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
      await api.setMonitored(id, !series.monitored)
      await Promise.all([detail.reload(), episodes.reload()])
    } finally {
      setSeriesMonitorPending(false)
    }
  }

  const tone = seriesTone(series.title)
  const heroBackground = manage ? `color-mix(in srgb, ${tone} 70%, var(--bg))` : tone
  const upNext = list?.up_next != null ? list.entries[list.up_next] : null
  const resuming = (upNext?.watch?.position_seconds ?? 0) > 0
  const next = series.next_airing
  const meta = [
    series.year,
    series.series_status ? capitalize(series.series_status) : null,
    next?.airs_at ? `New episode ${dayTimeLabel(next.airs_at)}` : null,
  ]
    .filter(Boolean)
    .join(' · ')
  const counts = list?.counts ?? series.counts
  const currentView = series.views.find((v) => v.id === (view ?? list?.view))

  return (
    <>
      <section
        aria-labelledby="series-title"
        className={`${styles.hero} ${manage ? styles.manage : ''}`}
        style={{ background: heroBackground }}
      >
        <div className={styles.heroBar}>
          <Link to="/tv" className={styles.back}>
            <BackIcon />
            TV Shows
          </Link>
        </div>
        <div className={styles.heroBody}>
          {manage && <span className={styles.managing}>Managing · changes apply for everyone</span>}
          <h1 id="series-title" className={styles.title}>
            {series.title}
          </h1>
          {meta && <p className={styles.heroMeta}>{meta}</p>}
          {!manage && (
            <>
              {series.overview && <p className={styles.overview}>{series.overview}</p>}
              <div className={styles.actions}>
                {upNext && (
                  <button type="button" className={styles.primary} title={PLAY_NOT_BUILT}>
                    <PlayIcon />
                    {resuming ? 'Resume' : 'Play'} {episodeLabel(upNext.number)}
                  </button>
                )}
                <button type="button" className={styles.secondary} title={PLAY_NOT_BUILT}>
                  Watch from start
                </button>
              </div>
            </>
          )}
          {manage && (
            <>
              <div className={styles.counts}>
                <span className={styles.count}>
                  <span className={styles.dot} style={{ background: 'var(--state-available)' }} />
                  <b>{counts.available}</b> available
                </span>
                <span className={styles.count}>
                  <span className={styles.dot} style={{ background: 'var(--state-missing)' }} />
                  <b>{counts.missing}</b> missing
                </span>
                <span className={styles.count}>
                  <span
                    className={styles.dot}
                    style={{ border: '1.5px solid var(--state-neutral)' }}
                  />
                  <b>{counts.not_acquired}</b> not acquired
                </span>
                <span className={styles.count}>
                  <span className={styles.dot} style={{ background: 'var(--accent)' }} />
                  <b>{counts.wanted}</b> wanted
                </span>
              </div>
              <div className={styles.manageActions}>
                <button type="button" disabled title="This isn't built yet">
                  Edit details
                </button>
                <button type="button" disabled title="This isn't built yet">
                  Rescan files
                </button>
                <button type="button" disabled title="This isn't built yet">
                  Refresh metadata
                </button>
              </div>
            </>
          )}
        </div>
      </section>

      <div className={styles.content}>
        {manage && (
          <ManagePanels
            series={series as SeriesDetail}
            monitorPending={seriesMonitorPending}
            onToggleMonitor={toggleSeriesMonitor}
          />
        )}

        <div className={styles.toolbar}>
          <div className={styles.toolbarStart}>
            <h2>Episodes</h2>
            {series.views.length > 1 && (
              <Segmented
                label="Ordering"
                variant="pills"
                value={view ?? list?.view}
                onChange={pickView}
                options={series.views.map((v) => ({ value: v.id, label: v.name }))}
              />
            )}
          </div>
          <Segmented
            label="Layout"
            variant="box"
            value={layout}
            onChange={setLayout}
            options={[
              { value: 'list', label: 'List', icon: <ListIcon /> },
              { value: 'grid', label: 'Grid', icon: <GridIcon /> },
            ]}
          />
        </div>
        <p className={styles.viewNote}>{viewNote(currentView, series.views)}</p>

        {episodes.error && <Failed error={episodes.error} onRetry={episodes.reload} />}
        {!list && !episodes.error && <Loading label="Loading episodes…" />}
        {list && (
          <Episodes
            entries={list.entries}
            upNext={list.up_next}
            mode={mode}
            layout={layout}
            pendingMonitor={pendingMonitor}
            onToggleMonitor={toggleEpisodeMonitor}
          />
        )}
      </div>
    </>
  )
}
