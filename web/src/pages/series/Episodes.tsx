import type { Episode } from '../../api/client'
import { Badge } from '../../components/Badge'
import { MoreIcon } from '../../components/Icons'
import { Progress } from '../../components/Progress'
import type { Layout, Mode } from '../../lib/prefs'
import { episodeView, type EpisodeView } from './episodeView'
import styles from './Episodes.module.css'

interface Props {
  entries: Episode[]
  upNext: number | null | undefined
  mode: Mode
  layout: Layout
  /** Episodes whose monitor change is in flight, with the value being saved. */
  pendingMonitor: Map<number, boolean>
  onToggleMonitor: (episode: Episode) => void
}

export function Episodes({
  entries,
  upNext,
  mode,
  layout,
  pendingMonitor,
  onToggleMonitor,
}: Props) {
  const views = entries.map((e, i) => episodeView(e, i, upNext, mode))
  const monitored = (v: EpisodeView) => pendingMonitor.get(v.episode.item_id) ?? v.episode.monitored
  return layout === 'list' ? (
    <EpisodeList views={views} mode={mode} monitored={monitored} onToggle={onToggleMonitor} />
  ) : (
    <EpisodeGrid views={views} mode={mode} monitored={monitored} onToggle={onToggleMonitor} />
  )
}

interface ViewProps {
  views: EpisodeView[]
  mode: Mode
  monitored: (v: EpisodeView) => boolean
  onToggle: (episode: Episode) => void
}

const numberColor = (v: EpisodeView) => (v.hasArt ? 'rgba(255, 255, 255, 0.92)' : '#5C6270')

function EpisodeList({ views, mode, monitored, onToggle }: ViewProps) {
  const manage = mode === 'manage'
  const items = []
  let lastSeason: number | null = null
  for (const v of views) {
    const number = v.episode.number
    if (number?.style === 'season' && number.season != null && number.season !== lastSeason) {
      lastSeason = number.season
      items.push(
        <li key={`season-${number.season}`} className={styles.season}>
          {number.season === 0 ? 'Specials' : `Season ${number.season}`}
        </li>,
      )
    }
    const title = v.episode.title
    items.push(
      <li key={v.key} className={`${styles.row} ${v.isUpNext && !manage ? styles.upNext : ''}`}>
        {manage && (
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={monitored(v)}
              onChange={() => onToggle(v.episode)}
              aria-label={`Monitor ${title}`}
            />
          </label>
        )}
        <div className={styles.thumb} style={{ background: v.tone }}>
          <span className={styles.thumbNumber} style={{ color: numberColor(v) }}>
            {v.label}
          </span>
          <Progress value={v.progress} />
        </div>
        <div className={styles.text}>
          <div className={styles.titleLine}>
            <span className={styles.title}>{title}</span>
            {v.episode.is_filler && <span className={styles.tag}>Filler</span>}
            {manage && v.episode.is_gap && <span className={styles.gap}>Gap</span>}
          </div>
          <span className={styles.meta}>{v.meta}</span>
          {manage && <span className={styles.tech}>{v.tech}</span>}
        </div>
        <Badge badge={v.badge} />
        {manage && (
          <button
            type="button"
            className={styles.more}
            aria-label={`More actions for ${title}`}
            title="Episode actions aren't built yet"
            disabled
          >
            <MoreIcon />
          </button>
        )}
      </li>,
    )
  }
  return <ol className={styles.list}>{items}</ol>
}

function EpisodeGrid({ views, mode, monitored, onToggle }: ViewProps) {
  const manage = mode === 'manage'
  return (
    <div className={styles.grid}>
      {views.map((v) => (
        <div key={v.key} className={styles.card}>
          <div
            className={`${styles.cardArt} ${v.isUpNext && !manage ? styles.upNext : ''}`}
            style={{ background: v.tone }}
          >
            <div className={styles.cardTop}>
              <Badge badge={v.badge} small />
              {manage && (
                <label className={styles.cardCheck}>
                  <input
                    type="checkbox"
                    checked={monitored(v)}
                    onChange={() => onToggle(v.episode)}
                  />
                  Monitored
                </label>
              )}
            </div>
            <div>
              <span className={styles.cardNumber} style={{ color: numberColor(v) }}>
                {v.label}
              </span>
              <Progress value={v.progress} />
            </div>
          </div>
          <span className={styles.cardText}>
            <span className={styles.title}>{v.episode.title}</span>
            <span className={styles.meta}>{v.meta}</span>
            {manage && <span className={styles.tech}>{v.tech}</span>}
          </span>
        </div>
      ))}
    </div>
  )
}
