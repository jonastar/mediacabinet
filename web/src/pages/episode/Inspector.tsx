import { useState } from 'react'
import { api, type EventRecord, type LinkedFile, type OrderingNumber } from '../../api/client'
import { useApi } from '../../api/useApi'
import { bytes, dayTimeLabel, resolution } from '../../lib/format'
import { useLiveEvents } from '../../lib/live'
import { ActivityList } from '../../components/ActivityFeed'
import { Segmented } from '../../components/Segmented'
import { Streams } from '../../components/Streams'
import styles from './Inspector.module.css'

const pad = (n: number) => String(n).padStart(2, '0')

/** `aired S01E08`, `absolute 8`. */
function position(n: OrderingNumber) {
  if (n.number == null) {
    return `${n.ordering} —`
  }
  const label = n.season != null ? `S${pad(n.season)}E${pad(n.number)}` : String(n.number)
  return `${n.ordering} ${label}`
}

function linkMeta(f: LinkedFile) {
  const source = f.link_source === 'user' ? 'user' : `${f.link_source} ${f.confidence.toFixed(2)}`
  return [
    `edition: ${f.edition ?? '—'}`,
    [resolution(f.height), f.video_codec, f.hdr_format].filter(Boolean).join(' '),
    f.status === 'present' ? bytes(f.size) : f.status,
    source,
    f.pinned ? 'pinned' : null,
  ]
    .filter(Boolean)
    .join(' · ')
}

/** One line for an event, naming files by path when they're still linked. */
function describe(e: EventRecord, paths: Map<number, string>) {
  const p = e.payload as Record<string, unknown>
  const file = typeof p.file_id === 'number' ? (paths.get(p.file_id) ?? `file ${p.file_id}`) : null
  const detail = (() => {
    switch (e.type) {
      case 'link.created':
        return [p.source, typeof p.confidence === 'number' ? p.confidence.toFixed(2) : null, file]
      case 'link.changed':
        return [`edition ${p.edition ?? 'cleared'}`, file]
      case 'item.edited':
        return [
          Array.isArray(p.fields) ? p.fields.join(', ') : Array.isArray(p.tags) ? 'tags' : null,
        ]
      case 'item.wanted':
        return [p.reason]
      case 'status.changed':
        return [p.source, p.status ?? 'cleared']
      default:
        return [typeof p.path === 'string' ? p.path : file]
    }
  })()
  return [e.type, ...detail].filter(Boolean).join(' ')
}

function LinkRow({
  file,
  itemId,
  onChanged,
}: {
  file: LinkedFile
  itemId: number
  onChanged: () => void
}) {
  const [edition, setEdition] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const run = async (action: () => Promise<void>) => {
    setError(null)
    try {
      await action()
      setEdition(null)
      onChanged()
    } catch (e) {
      setError((e as Error).message)
    }
  }
  const unlink = () => {
    if (
      window.confirm(`Unlink ${file.relative_path}? Scans won't link it to this episode again.`)
    ) {
      run(() => api.unlinkFile(file.file_id, itemId))
    }
  }

  return (
    <li className={styles.link}>
      <span className={styles.path}>
        {file.root_path}/{file.relative_path}
      </span>
      <span className={styles.meta}>{linkMeta(file)}</span>
      <Streams fileId={file.file_id} />
      {edition === null ? (
        <div className={styles.actions}>
          <button type="button" className={styles.button} onClick={unlink}>
            Unlink
          </button>
          <button
            type="button"
            className={styles.button}
            onClick={() => setEdition(file.edition ?? '')}
          >
            Set edition
          </button>
        </div>
      ) : (
        <form
          className={styles.actions}
          onSubmit={(e) => {
            e.preventDefault()
            run(() => api.setEdition(file.file_id, itemId, edition || null))
          }}
        >
          <input
            className={styles.input}
            aria-label="Edition"
            placeholder="Director's Cut (empty: the usual one)"
            value={edition}
            onChange={(e) => setEdition(e.target.value)}
            autoFocus
          />
          <button type="submit" className={styles.button}>
            Save
          </button>
          <button type="button" className={styles.button} onClick={() => setEdition(null)}>
            Cancel
          </button>
        </form>
      )}
      {error && <span className={styles.error}>{error}</span>}
    </li>
  )
}

/**
 * Manage mode's close look at one episode: every linked file, its number in each
 * ordering, and what has happened to it.
 */
export function Inspector({
  itemId,
  files,
  onChanged,
}: {
  itemId: number
  files: LinkedFile[]
  onChanged: () => void
}) {
  const item = useApi(`item:${itemId}`, () => api.item(itemId))
  const events = useApi(`item-events:${itemId}`, () => api.itemEvents(itemId))
  const activity = useApi(`item-activity:${itemId}`, () => api.activity({ item_id: itemId }))
  const [history, setHistory] = useState<'activity' | 'events'>('activity')
  const [copied, setCopied] = useState(false)
  useLiveEvents(['activity.added'], activity.reload)
  useLiveEvents(
    [
      'file.added',
      'file.changed',
      'file.missing',
      'file.moved',
      'file.restored',
      'file.removed',
      'file.upgraded',
      'link.created',
      'link.changed',
      'link.removed',
      'item.edited',
      'item.wanted',
      'item.unwanted',
      'status.changed',
      'series.refreshed',
    ],
    () => Promise.all([item.reload(), events.reload()]),
  )
  const paths = new Map(files.map((f) => [f.file_id, f.relative_path]))
  const ref = `item:${itemId}`
  const ids = item.data?.external_ids.map((x) => `${x.provider}:${x.value}`) ?? []
  let positions = '…'
  if (item.error) {
    positions = item.error.message
  } else if (item.data) {
    positions = item.data.numbers.map(position).join(' · ') || 'In no ordering.'
  }

  return (
    <section aria-labelledby="inspector-title" className={styles.inspector}>
      <div className={styles.header}>
        <h2 id="inspector-title">Inspector</h2>
        <span className={styles.ids}>{[ref, ...ids].join(' · ')}</span>
        <button
          type="button"
          className={styles.button}
          onClick={() => navigator.clipboard?.writeText(ref).then(() => setCopied(true))}
        >
          {copied ? 'Copied' : 'Copy API ref'}
        </button>
      </div>

      <div className={styles.block}>
        <h3>Links ({files.length})</h3>
        {files.length === 0 ? (
          <p className={styles.empty}>No files linked.</p>
        ) : (
          <ul className={styles.links}>
            {files.map((f) => (
              <LinkRow key={f.file_id} file={f} itemId={itemId} onChanged={onChanged} />
            ))}
          </ul>
        )}
      </div>

      <div className={styles.block}>
        <h3>Positions</h3>
        <p className={styles.mono}>{positions}</p>
      </div>

      <div className={styles.block}>
        <div className={styles.historyHead}>
          <h3>History</h3>
          <Segmented
            label="History"
            variant="pills"
            value={history}
            onChange={setHistory}
            options={[
              { value: 'activity', label: 'Activity' },
              { value: 'events', label: 'Events' },
            ]}
          />
        </div>
        {history === 'activity' && activity.error && (
          <p className={styles.error}>{activity.error.message}</p>
        )}
        {history === 'activity' && activity.data && (
          <ActivityList entries={activity.data.entries} empty="Nothing yet." />
        )}
        {history === 'events' && events.error && (
          <p className={styles.error}>{events.error.message}</p>
        )}
        {history === 'events' && events.data?.length === 0 && (
          <p className={styles.empty}>Nothing yet.</p>
        )}
        {history === 'events' && events.data && events.data.length > 0 && (
          <ol className={styles.events}>
            {events.data.map((e) => (
              <li key={e.id} className={styles.mono}>
                <time dateTime={new Date(e.created_at * 1000).toISOString()}>
                  {dayTimeLabel(e.created_at)}
                </time>{' '}
                {describe(e, paths)}
              </li>
            ))}
          </ol>
        )}
      </div>
    </section>
  )
}
