import { useState } from 'react'
import { api, type MovePlan, type SeriesDetail } from '../../api/client'
import { useApi } from '../../api/useApi'
import styles from './ManagePanels.module.css'

type Action = { kind: 'organize' } | { kind: 'move'; rootId: number }

/** Organise files to the template, or move the series to another root, with a preview first. */
export function LocationActions({
  series,
  onDone,
}: {
  series: SeriesDetail
  onDone: () => Promise<unknown>
}) {
  const libraries = useApi('libraries', api.libraries)
  const [action, setAction] = useState<Action | null>(null)
  const [plan, setPlan] = useState<MovePlan | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const current = new Set(series.locations.map((l) => l.root_id))
  const targets = (libraries.data ?? [])
    .filter((l) => l.kind === 'tv')
    .flatMap((l) => l.roots)
    .filter((r) => r.write_policy !== 'read_only' && r.online && !current.has(r.id))
  const managed = series.locations.some((l) => l.write_policy === 'managed')

  const run = (a: Action, dryRun: boolean) =>
    a.kind === 'organize'
      ? api.organize(series.id, dryRun)
      : api.moveSeries(series.id, a.rootId, dryRun)

  const preview = async (a: Action) => {
    setAction(a)
    setPlan(null)
    setError(null)
    try {
      setPlan(await run(a, true))
    } catch (e) {
      setError((e as Error).message)
    }
  }

  const apply = async () => {
    if (!action) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      const done = await run(action, false)
      setPlan(done)
      await onDone()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
        {managed && (
          <button
            type="button"
            className={styles.button}
            onClick={() => preview({ kind: 'organize' })}
          >
            Organize files…
          </button>
        )}
        {targets.length > 0 && (
          <label className={styles.field} style={{ flex: '1 1 100%' }}>
            Move series to
            <select
              value={action?.kind === 'move' ? action.rootId : ''}
              onChange={(e) =>
                e.target.value && preview({ kind: 'move', rootId: Number(e.target.value) })
              }
            >
              <option value="">Choose a folder…</option>
              {targets.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.path}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
      {error && <span className={styles.warning}>{error}</span>}
      {plan && (
        <div className={styles.plan}>
          {plan.moves.length === 0 && plan.skipped.length === 0 && (
            <span className={styles.muted}>Nothing to do: every file is already in place.</span>
          )}
          {plan.moves.length > 0 && (
            <span className={styles.muted}>
              {plan.applied ? 'Moved' : 'Will move'} {plan.moves.length}{' '}
              {plan.moves.length === 1 ? 'file' : 'files'}:
            </span>
          )}
          <ul>
            {plan.moves.slice(0, 8).map((m) => (
              <li key={m.file_id} className={styles.mono}>
                {m.to}
              </li>
            ))}
            {plan.moves.length > 8 && (
              <li className={styles.muted}>…and {plan.moves.length - 8} more</li>
            )}
          </ul>
          {plan.skipped.map((s) => (
            <span key={s} className={styles.muted}>
              Skipped: {s}
            </span>
          ))}
          {!plan.applied && plan.moves.length > 0 && (
            <div style={{ display: 'flex', gap: 8 }}>
              <button
                type="button"
                className={`${styles.button} ${styles.primary}`}
                disabled={busy}
                onClick={apply}
              >
                {busy ? 'Moving…' : action?.kind === 'organize' ? 'Rename files' : 'Move files'}
              </button>
              <button type="button" className={styles.button} onClick={() => setPlan(null)}>
                Cancel
              </button>
            </div>
          )}
        </div>
      )}
    </>
  )
}
