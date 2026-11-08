import { useState, type ReactNode } from 'react'
import type { Episode, LinkedFile } from '../../api/client'
import { Badge } from '../../components/Badge'
import { Link } from '../../components/Link'
import { airDate, bytes, fileLabel, runtime } from '../../lib/format'
import { episodeView, groupBySeason, type EpisodeView } from './episodeView'
import styles from './Episodes.module.css'

const present = (e: Episode) => e.files.filter((f: LinkedFile) => f.status === 'present')

function size(e: Episode) {
  const files = present(e)
  return files.length > 0 ? bytes(files.reduce((sum, f) => sum + f.size, 0)) : '—'
}

function matches(v: EpisodeView, filter: string) {
  const text = `${v.label} ${v.episode.title}`.toLowerCase()
  return text.includes(filter.trim().toLowerCase())
}

/** The Episodes section in edit mode: a dense table with bulk actions. */
export function EpisodeTable({
  entries,
  upNext,
  episodeLink,
  pendingMonitor,
  onToggleMonitor,
  onSetPlayed,
  onAddToPlaylist,
  order,
  initialSeason,
  note,
  status,
}: {
  entries: Episode[]
  upNext: number | null | undefined
  episodeLink: (episode: Episode) => string
  /** Episodes whose monitor change is in flight, with the value being saved. */
  pendingMonitor: Map<number, boolean>
  onToggleMonitor: (episode: Episode) => void
  onSetPlayed: (episodes: Episode[], played: boolean) => void
  onAddToPlaylist: (episodes: Episode[]) => void
  order: ReactNode
  /** The season to show first, when a link asked for one. */
  initialSeason: string | null
  note: ReactNode
  status?: ReactNode
}) {
  const [picked, setPicked] = useState(initialSeason)
  const [filter, setFilter] = useState('')
  const [selected, setSelected] = useState(new Set<number>())

  const groups = groupBySeason(entries.map((e, i) => episodeView(e, i, upNext, 'manage')))
  const seasons = groups.filter((g) => g.season !== null)
  const season = groups.some((g) => g.key === picked) || picked === 'all' ? picked : groups[0]?.key
  const inSeason = season === 'all' ? groups : groups.filter((g) => g.key === season)
  const rows = inSeason.flatMap((g) => g.views).filter((v) => matches(v, filter))
  const chosen = entries.filter((e) => selected.has(e.item_id))
  const allChosen = rows.length > 0 && rows.every((v) => selected.has(v.episode.item_id))

  const choose = (ids: number[], on: boolean) => {
    setSelected((current) => {
      const next = new Set(current)
      for (const id of ids) {
        if (on) {
          next.add(id)
        } else {
          next.delete(id)
        }
      }
      return next
    })
  }

  return (
    <section aria-labelledby="episodes-title" className={styles.section}>
      <div className={styles.toolbar}>
        <div className={styles.toolbarStart}>
          <h2 id="episodes-title">Episodes</h2>
          {seasons.length > 1 && (
            <select
              className={styles.select}
              aria-label="Season"
              value={season ?? ''}
              onChange={(e) => setPicked(e.target.value)}
            >
              {seasons.map((g) => (
                <option key={g.key} value={g.key}>
                  {g.name}
                </option>
              ))}
              <option value="all">All seasons</option>
            </select>
          )}
          {order}
          <input
            className={styles.filter}
            aria-label="Filter episodes"
            placeholder="Filter episodes"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        {chosen.length > 0 && (
          <div className={styles.bulkBar} role="toolbar" aria-label="Selected episodes">
            <span className={styles.bulkCount}>{chosen.length} selected</span>
            <button
              type="button"
              className={styles.bulkAction}
              onClick={() => onSetPlayed(chosen, true)}
            >
              Mark watched
            </button>
            <button
              type="button"
              className={styles.bulkAction}
              onClick={() => onAddToPlaylist(chosen)}
            >
              Add to playlist
            </button>
            <button
              type="button"
              className={styles.bulkAction}
              disabled
              title="Metadata refreshes a whole series: use Refresh metadata at the top"
            >
              Refresh metadata
            </button>
          </div>
        )}
      </div>
      {note}

      {status ?? (
        <div className={styles.tableBox}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col" className={styles.checkCell}>
                  <input
                    type="checkbox"
                    aria-label="Select all"
                    checked={allChosen}
                    onChange={() =>
                      choose(
                        rows.map((v) => v.episode.item_id),
                        !allChosen,
                      )
                    }
                  />
                </th>
                <th scope="col">#</th>
                <th scope="col">Title</th>
                <th scope="col">Aired</th>
                <th scope="col">Runtime</th>
                <th scope="col">File</th>
                <th scope="col">Size</th>
                <th scope="col">Monitored</th>
                <th scope="col" className={styles.statusCell}>
                  Status
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((v) => {
                const e = v.episode
                const on = selected.has(e.item_id)
                const files = present(e)
                return (
                  <tr key={v.key} className={on ? styles.tableRowOn : undefined}>
                    <td className={styles.checkCell}>
                      <input
                        type="checkbox"
                        aria-label={`Select ${v.label}`}
                        checked={on}
                        onChange={() => choose([e.item_id], !on)}
                      />
                    </td>
                    <td className={styles.codeCell}>{v.label}</td>
                    <td className={styles.titleCell}>
                      <Link to={episodeLink(e)} className={styles.title}>
                        {e.title}
                      </Link>
                      {e.is_filler && <span className={styles.tag}>Filler</span>}
                      {e.is_gap && <span className={styles.gap}>Gap</span>}
                    </td>
                    <td>{airDate(e.airs_at, e.airs_time_known)}</td>
                    <td>{runtime(e.runtime_seconds) ?? '—'}</td>
                    <td>
                      <span
                        className={`${styles.fileChip} ${files.length ? '' : styles.noFile}`}
                        title={v.tech}
                      >
                        {files[0] ? fileLabel(files[0]) : 'No file'}
                        {files.length > 1 && ` +${files.length - 1}`}
                      </span>
                    </td>
                    <td className={styles.numeric}>{size(e)}</td>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`Monitor ${e.title}`}
                        checked={pendingMonitor.get(e.item_id) ?? e.monitored}
                        onChange={() => onToggleMonitor(e)}
                      />
                    </td>
                    <td className={styles.statusCell}>
                      <Badge badge={v.badge} small />
                    </td>
                  </tr>
                )
              })}
              {rows.length === 0 && (
                <tr>
                  <td colSpan={9} className={styles.empty}>
                    No episodes match.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}
