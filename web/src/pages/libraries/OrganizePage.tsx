import { useState } from 'react'
import {
  api,
  type FailedMove,
  type Library,
  type OrganizeGroup,
  type OrganizePreview,
  type PlannedMove,
} from '../../api/client'
import { useApi } from '../../api/useApi'
import { BackIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { Failed, Loading } from '../../components/Status'
import { LOCALE } from '../../core/locale'
import { navigate, useQueryParam } from '../../lib/router'
import { NotFoundPage } from '../NotFoundPage'
import { useLibraries } from './useLibraries'
import styles from './Libraries.module.css'
import own from './OrganizePage.module.css'

/** Moves sent per request, so a big library shows progress and no request runs long. */
const BATCH = 100

const count = (n: number, noun: string) =>
  `${n.toLocaleString(LOCALE)} ${noun}${n === 1 ? '' : 's'}`

/** Where a file is and where it goes, sharing the start of the path they have in common. */
function MoveLine({ move }: { move: PlannedMove }) {
  const from = move.from.split('/')
  const to = move.to.split('/')
  let same = 0
  while (same < from.length - 1 && same < to.length - 1 && from[same] === to[same]) {
    same += 1
  }
  const prefix = same > 0 ? `${from.slice(0, same).join('/')}/` : ''
  return (
    <span className={own.paths}>
      <span className={own.from}>
        {prefix && <span className={own.prefix}>{prefix}</span>}
        {from.slice(same).join('/')}
      </span>
      <span className={own.to}>
        <span aria-hidden="true">→ </span>
        <span className="visually-hidden">to </span>
        {prefix && <span className={own.prefix}>{prefix}</span>}
        {to.slice(same).join('/')}
      </span>
    </span>
  )
}

function Group({
  group,
  chosen,
  failed,
  disabled,
  onToggle,
}: {
  group: OrganizeGroup
  chosen: Set<number>
  failed: Map<number, string>
  disabled: boolean
  onToggle: (ids: number[], on: boolean) => void
}) {
  const ids = group.moves.map((m) => m.file_id)
  const picked = ids.filter((id) => chosen.has(id)).length
  const label = group.year ? `${group.title} (${group.year})` : group.title
  return (
    <li className={own.group}>
      <div className={own.groupHead}>
        {ids.length > 0 ? (
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={picked === ids.length}
              ref={(el) => {
                if (el) {
                  el.indeterminate = picked > 0 && picked < ids.length
                }
              }}
              disabled={disabled}
              onChange={(e) => onToggle(ids, e.target.checked)}
            />
            <b>{label}</b>
          </label>
        ) : (
          <b className={own.title}>{label}</b>
        )}
        <span className={styles.muted}>
          {[group.root_path, ids.length ? count(ids.length, 'change') : null]
            .filter(Boolean)
            .join(' · ')}
        </span>
      </div>
      {ids.length > 0 && (
        <ul className={own.moves}>
          {group.moves.map((m) => (
            <li key={m.file_id}>
              <label className={own.move}>
                <input
                  type="checkbox"
                  checked={chosen.has(m.file_id)}
                  disabled={disabled}
                  onChange={(e) => onToggle([m.file_id], e.target.checked)}
                />
                <MoveLine move={m} />
              </label>
              {failed.has(m.file_id) && (
                <span className={styles.error}>{failed.get(m.file_id)}</span>
              )}
            </li>
          ))}
        </ul>
      )}
      {group.skipped.length === 1 && (
        <span className={own.skipped}>Left as is: {group.skipped[0]}</span>
      )}
      {group.skipped.length > 1 && (
        <details className={own.skipped}>
          <summary>{count(group.skipped.length, 'file')} left as is</summary>
          <ul className={own.moves}>
            {group.skipped.map((s) => (
              <li key={s}>{s}</li>
            ))}
          </ul>
        </details>
      )}
    </li>
  )
}

function Plan({ library, rootId }: { library: Library; rootId: number | null }) {
  const scope = rootId ? { root_id: rootId } : { library_id: library.id }
  // Each round is a fresh preview, after changes were applied.
  const [round, setRound] = useState(0)
  const preview = useApi(`organize:${library.id}:${rootId ?? 'all'}:${round}`, () =>
    api.previewOrganize(scope),
  )
  if (preview.error) {
    return <Failed error={preview.error} onRetry={preview.reload} />
  }
  if (!preview.data || preview.loading) {
    return <Loading />
  }
  // Keyed by round, so a new preview starts with everything chosen again.
  return <Changes key={round} preview={preview.data} onDone={() => setRound((n) => n + 1)} />
}

function Changes({ preview, onDone }: { preview: OrganizePreview; onDone: () => void }) {
  const moves = preview.groups.flatMap((g) => g.moves)
  const [chosen, setChosen] = useState(() => new Set(moves.map((m) => m.file_id)))
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null)
  const [failed, setFailed] = useState<Map<number, string>>(new Map())
  const [result, setResult] = useState<{ moved: number; failed: FailedMove[] } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const busy = progress != null

  const toggle = (ids: number[], on: boolean) => {
    setChosen((current) => {
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

  const apply = async () => {
    const todo = moves.filter((m) => chosen.has(m.file_id))
    setError(null)
    setResult(null)
    setProgress({ done: 0, total: todo.length })
    let moved = 0
    const failures: FailedMove[] = []
    try {
      for (let i = 0; i < todo.length; i += BATCH) {
        const done = await api.applyOrganize(todo.slice(i, i + BATCH))
        moved += done.moved.length
        failures.push(...done.failed)
        setProgress({ done: Math.min(i + BATCH, todo.length), total: todo.length })
      }
    } catch (e) {
      setError((e as Error).message)
    }
    setProgress(null)
    setFailed(new Map(failures.map((f) => [f.file_id, f.error])))
    setResult({ moved, failed: failures })
    if (failures.length === 0) {
      onDone()
    }
  }

  const skipped = preview.groups.reduce((n, g) => n + g.skipped.length, 0)
  const summary = [
    count(preview.files, 'file'),
    `${preview.in_place.toLocaleString(LOCALE)} already in place`,
    moves.length ? `${count(moves.length, 'to rename')}` : null,
    skipped ? `${skipped.toLocaleString(LOCALE)} left as is` : null,
  ]
    .filter(Boolean)
    .join(' · ')

  return (
    <>
      <p className={styles.muted} role="status">
        {summary}
      </p>
      {moves.length === 0 && skipped === 0 && (
        <div className={`${styles.fit} ${styles.fitOk}`} role="status">
          <strong>Everything is already where the layout puts it.</strong>
        </div>
      )}
      {result && (
        <div
          className={`${styles.fit} ${result.failed.length ? styles.fitPartial : styles.fitOk}`}
          role="status"
        >
          <strong>
            Renamed {count(result.moved, 'file')}.
            {result.failed.length > 0 && ` ${count(result.failed.length, 'file')} couldn’t be.`}
          </strong>
          {result.failed.length > 0 && (
            <span>
              Each is marked below. Preview again to see what’s left.{' '}
              <button type="button" className={styles.link} onClick={onDone}>
                Preview again
              </button>
            </span>
          )}
        </div>
      )}
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}

      {moves.length > 0 && (
        <div className={own.bar}>
          <button
            type="button"
            className={`${styles.button} ${styles.primary}`}
            disabled={busy || chosen.size === 0}
            onClick={apply}
          >
            {progress
              ? `Renaming… ${progress.done.toLocaleString(LOCALE)} of ${progress.total.toLocaleString(LOCALE)}`
              : `Rename ${count(chosen.size, 'file')}`}
          </button>
          <button
            type="button"
            className={`${styles.button} ${styles.small}`}
            disabled={busy}
            onClick={() =>
              setChosen(
                chosen.size === moves.length ? new Set() : new Set(moves.map((m) => m.file_id)),
              )
            }
          >
            {chosen.size === moves.length ? 'Choose none' : 'Choose all'}
          </button>
          <span className={styles.muted}>
            Sidecars move with their video. Nothing is overwritten.
          </span>
        </div>
      )}

      {preview.groups.length > 0 && (
        <ul className={own.groups}>
          {preview.groups.map((g) => (
            <Group
              key={`${g.item_id}:${g.root_id}`}
              group={g}
              chosen={chosen}
              failed={failed}
              disabled={busy}
              onToggle={toggle}
            />
          ))}
        </ul>
      )}
    </>
  )
}

/**
 * Organise: rename and move a library's files to their folder's layout. Shows every
 * change first, each with a checkbox, and applies only what's chosen.
 */
export function OrganizePage({ libraryId }: { libraryId: number }) {
  const libraries = useLibraries()
  const root = useQueryParam('root')
  if (libraries.error) {
    return <Failed error={libraries.error} onRetry={libraries.reload} />
  }
  if (!libraries.data) {
    return <Loading />
  }
  const library = libraries.data.find((l) => l.id === libraryId)
  if (!library || library.kind === 'folders') {
    return <NotFoundPage />
  }
  const writable = library.roots.filter((r) => !r.read_only)
  const rootId = writable.some((r) => String(r.id) === root) ? Number(root) : null
  const base = `/manage/libraries/${library.id}/organize`

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <Link to={`/manage/libraries/${library.id}`} className={styles.back}>
          <BackIcon />
          {library.name}
        </Link>
        <div className={styles.heading}>
          <h1>Organise</h1>
          <p className={styles.lead}>
            Rename and move files to match their folder’s layout. Untick anything you want to leave
            where it is. Read-only folders are never changed.
          </p>
        </div>
        {writable.length === 0 ? (
          <p className={styles.muted}>
            Every folder of {library.name} is read-only, so there’s nothing Organise may change.
          </p>
        ) : (
          <>
            <label className={styles.field} htmlFor="organize-scope">
              Look at
              <select
                id="organize-scope"
                className={styles.input}
                value={rootId ?? ''}
                onChange={(e) =>
                  navigate(e.target.value ? `${base}?root=${e.target.value}` : base, {
                    replace: true,
                  })
                }
              >
                <option value="">Every folder of {library.name}</option>
                {writable.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.path}
                  </option>
                ))}
              </select>
            </label>
            <Plan key={rootId ?? 'all'} library={library} rootId={rootId} />
          </>
        )}
      </div>
    </div>
  )
}
