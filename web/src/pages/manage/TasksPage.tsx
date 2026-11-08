import { useEffect, useState } from 'react'
import { api, ApiError, type TaskSummary } from '../../api/client'
import { useApi } from '../../api/useApi'
import { BackIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { Progress } from '../../components/Progress'
import { Failed, Loading } from '../../components/Status'
import { ago, dayTimeLabel } from '../../lib/format'
import { NotFoundPage } from '../NotFoundPage'
import buttons from '../libraries/Libraries.module.css'
import manage from './Manage.module.css'
import styles from './Tasks.module.css'
import { LOCALE } from '../../core/locale'

const KINDS: Record<string, string> = {
  scan: 'Scan',
  metadata_refresh: 'Metadata refresh',
  thumbnails: 'Thumbnails',
  artwork: 'Artwork downloads',
}

const STATES: Record<string, string> = {
  queued: 'Queued',
  running: 'Running',
  ok: 'Finished',
  failed: 'Failed',
  cancelled: 'Cancelled',
}

const TRIGGERS: Record<string, string> = {
  added: 'Automatic',
  manual: 'By hand',
  scheduled: 'Schedule',
  watch: 'Folder watch',
}

const PAGE = 50
/** Quick while something's happening; otherwise just enough to notice new tasks. */
const BUSY_POLL = 1000
// Short enough that a task lasting seconds still shows as running.
const IDLE_POLL = 3000

function usePoll(reload: () => void, busy: boolean) {
  useEffect(() => {
    const timer = window.setInterval(reload, busy ? BUSY_POLL : IDLE_POLL)
    return () => window.clearInterval(timer)
  }, [reload, busy])
}

function scopeLabel(t: TaskSummary) {
  return t.root_path ?? t.library_name ?? 'Removed'
}

function took(seconds: number) {
  if (seconds < 60) {
    return `${seconds} s`
  }
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) {
    return `${minutes} min ${seconds % 60} s`
  }
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`
}

function duration(t: TaskSummary, now = Date.now() / 1000) {
  if (!t.started_at) {
    return null
  }
  return took(Math.max(0, Math.round((t.finished_at ?? now) - t.started_at)))
}

/** A finished task with a problem says Finished, but shouldn't look like all went well. */
function stateTone(t: TaskSummary) {
  if (t.state === 'ok' && t.error) {
    return 'problem'
  }
  return t.state
}

function StateBadge({ task }: { task: TaskSummary }) {
  return (
    <span className={`${styles.badge} ${styles[stateTone(task)]}`}>
      {stateTone(task) === 'problem' ? 'With problems' : (STATES[task.state] ?? task.state)}
    </span>
  )
}

function progressText(t: TaskSummary) {
  const p = t.progress
  if (t.state === 'queued') {
    return 'Waiting for its turn'
  }
  if (!p) {
    return null
  }
  const count =
    p.total > 0 ? `${p.done.toLocaleString(LOCALE)} of ${p.total.toLocaleString(LOCALE)}` : null
  return [p.cancelling ? 'stopping' : p.phase, count].filter(Boolean).join(' · ')
}

function TaskRow({ task: t }: { task: TaskSummary }) {
  const p = t.progress
  const detail = progressText(t) ?? (t.error || null)
  return (
    <li>
      <Link to={`/manage/tasks/${t.id}`} className={styles.row}>
        <span className={styles.rowMain}>
          <span className={styles.rowTitle}>
            <StateBadge task={t} />
            <strong>{KINDS[t.kind] ?? t.kind}</strong>
            <span className={styles.scope}>{scopeLabel(t)}</span>
          </span>
          {detail && <span className={styles.detail}>{detail}</span>}
        </span>
        <span className={styles.when}>
          {TRIGGERS[t.trigger] ?? t.trigger} ·{' '}
          {ago(t.finished_at ?? t.started_at ?? t.requested_at)}
          {duration(t) && ` · ${duration(t)}`}
        </span>
        {t.state === 'running' && p && p.total > 0 && (
          <span className={styles.bar}>
            <Progress value={p.done / p.total} />
          </span>
        )}
      </Link>
    </li>
  )
}

/** Everything the server does in the background: what's on now, and what happened. */
export function TasksPage() {
  const [kind, setKind] = useState('')
  const [state, setState] = useState('')
  const [limit, setLimit] = useState(PAGE)
  const tasks = useApi(`tasks:${kind}:${state}:${limit}`, () =>
    api.tasks({ kind: kind || undefined, state: state || undefined, limit }),
  )
  usePoll(tasks.reload, (tasks.data?.open.length ?? 0) > 0)

  if (tasks.error) {
    return <Failed error={tasks.error} onRetry={tasks.reload} />
  }
  if (!tasks.data) {
    return <Loading />
  }
  const { open, finished, has_more: hasMore } = tasks.data
  return (
    <div className={manage.page}>
      <div className={styles.columns}>
        <section aria-labelledby="tasks-open" className={styles.section}>
          <h1 id="tasks-open">Running now</h1>
          {open.length > 0 ? (
            <ul className={styles.list}>
              {open.map((t) => (
                <TaskRow key={t.id} task={t} />
              ))}
            </ul>
          ) : (
            <p className={styles.muted}>Nothing is running.</p>
          )}
        </section>

        <section aria-labelledby="tasks-history" className={styles.section}>
          <div className={styles.historyHead}>
            <h2 id="tasks-history">History</h2>
            <div className={styles.filters}>
              <select
                aria-label="Kind"
                className={styles.select}
                value={kind}
                onChange={(e) => setKind(e.target.value)}
              >
                <option value="">All kinds</option>
                {Object.entries(KINDS).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
              <select
                aria-label="Result"
                className={styles.select}
                value={state}
                onChange={(e) => setState(e.target.value)}
              >
                <option value="">All results</option>
                <option value="ok">Finished</option>
                <option value="failed">Failed</option>
                <option value="cancelled">Cancelled</option>
              </select>
            </div>
          </div>
          {finished.length > 0 ? (
            <ul className={styles.list}>
              {finished.map((t) => (
                <TaskRow key={t.id} task={t} />
              ))}
            </ul>
          ) : (
            <p className={styles.muted}>No tasks here yet.</p>
          )}
          {hasMore && (
            <button
              type="button"
              className={`${buttons.button} ${buttons.small} ${styles.more}`}
              onClick={() => setLimit(limit + PAGE)}
            >
              Show more
            </button>
          )}
        </section>
      </div>
    </div>
  )
}

interface Failure {
  path?: string
  title?: string
  error?: string
}

function failuresOf(outcome: unknown): Failure[] {
  const failures = (outcome as { failures?: unknown } | null | undefined)?.failures
  return Array.isArray(failures) ? (failures as Failure[]) : []
}

/** One task: where it stands, what went wrong, and what it logged. */
export function TaskInspector({ id }: { id: number }) {
  const task = useApi(`task:${id}`, () => api.task(id))
  const [cancelError, setCancelError] = useState<string | null>(null)
  const [cancelling, setCancelling] = useState(false)
  const open = task.data?.state === 'queued' || task.data?.state === 'running'
  usePoll(task.reload, open)

  if (task.error instanceof ApiError && task.error.status === 404) {
    return <NotFoundPage />
  }
  if (task.error) {
    return <Failed error={task.error} onRetry={task.reload} />
  }
  if (!task.data) {
    return <Loading />
  }
  const t = task.data

  const cancel = async () => {
    setCancelling(true)
    setCancelError(null)
    try {
      await api.cancelTask(id)
      await task.reload()
    } catch (e) {
      setCancelError((e as Error).message)
    } finally {
      setCancelling(false)
    }
  }

  const failures = failuresOf(t.outcome)
  const facts: [string, string][] = [
    ['Scope', scopeLabel(t)],
    ['Asked for by', TRIGGERS[t.trigger] ?? t.trigger],
    ['Requested', dayTimeLabel(t.requested_at)],
  ]
  if (t.started_at) {
    facts.push(['Started', dayTimeLabel(t.started_at)])
  }
  if (t.finished_at) {
    facts.push(['Finished', dayTimeLabel(t.finished_at)])
  }
  const ran = duration(t)
  if (ran) {
    facts.push(['Took', ran])
  }
  const progress = progressText(t)
  if (open && progress) {
    facts.push(['Progress', progress])
  }

  return (
    <div className={manage.page}>
      <div className={styles.columns}>
        <Link to="/manage/tasks" className={buttons.back}>
          <BackIcon />
          Tasks
        </Link>
        <div className={styles.inspectorHead}>
          <h1>
            {KINDS[t.kind] ?? t.kind} <span className={styles.taskId}>#{t.id}</span>
          </h1>
          <StateBadge task={t} />
          {open && (
            <button
              type="button"
              className={`${buttons.button} ${buttons.small} ${styles.cancel}`}
              disabled={!t.cancellable || cancelling || t.progress?.cancelling}
              title={t.cancellable ? undefined : 'A running scan can’t be stopped'}
              onClick={cancel}
            >
              {t.progress?.cancelling ? 'Stopping…' : 'Stop'}
            </button>
          )}
        </div>
        {cancelError && (
          <p className={styles.errorBox} role="alert">
            {cancelError}
          </p>
        )}
        <dl className={styles.facts}>
          {facts.map(([label, value]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
        {t.error && (
          <p className={styles.errorBox} role="status">
            {t.error}
          </p>
        )}

        {failures.length > 0 && (
          <section aria-labelledby="task-failures" className={styles.section}>
            <h2 id="task-failures">What went wrong</h2>
            <ul className={styles.failures}>
              {failures.map((f, i) => (
                <li key={i}>
                  <code>{f.path ?? f.title ?? 'Unknown'}</code>
                  <span>{f.error}</span>
                </li>
              ))}
            </ul>
          </section>
        )}

        <section aria-labelledby="task-log" className={styles.section}>
          <h2 id="task-log">Log</h2>
          {t.log.length > 0 ? (
            <ol className={styles.log}>
              {t.log.map((line, i) => (
                <li key={i} className={styles[`log_${line.level}`]}>
                  <span className={styles.logTime}>{dayTimeLabel(line.at)}</span>
                  <span className={styles.logLevel}>{line.level}</span>
                  <span className={styles.logMessage}>
                    {line.message}
                    {line.fields != null && (
                      <span className={styles.logFields}> {JSON.stringify(line.fields)}</span>
                    )}
                  </span>
                </li>
              ))}
            </ol>
          ) : (
            <p className={styles.muted}>Nothing logged.</p>
          )}
        </section>

        {t.outcome != null && (
          <details className={styles.outcome}>
            <summary>Outcome data</summary>
            <pre>{JSON.stringify(t.outcome, null, 2)}</pre>
          </details>
        )}
      </div>
    </div>
  )
}
