import type { ReactNode } from 'react'
import type { ActivityEntry } from '../api/client'
import { ago, dayTimeLabel, episodeLabel } from '../lib/format'
import { episodePath, itemPath } from '../lib/router'
import { useUser } from '../lib/session'
import { Link } from './Link'
import styles from './ActivityFeed.module.css'

type Payload = Record<string, unknown>

const TASK_KINDS: Record<string, string> = {
  scan: 'Scan task',
  metadata_refresh: 'Metadata refresh task',
  thumbnails: 'Thumbnails task',
}

const TARGETS: Record<string, string> = {
  library: 'the library',
  root: 'the folder',
  api_key: 'the API key',
  webhook: 'the webhook',
  sign_in_provider: 'the sign-in provider',
  sign_in: 'the sign-in settings',
}

const ACTIONS: Record<string, string> = {
  created: 'added',
  changed: 'changed',
  removed: 'removed',
  overridden: 'took over from the environment',
  from_env: 'handed back to the environment',
}

function ItemLink({ entry }: { entry: ActivityEntry }) {
  const item = entry.item
  if (!item) {
    return <>something since removed</>
  }
  if (item.kind === 'episode' && item.series_id != null) {
    return (
      <Link to={episodePath(item.series_id, item.id, item.library_id)}>
        {item.series_title} · {episodeLabel(item.number)} {item.title}
      </Link>
    )
  }
  const kind = item.kind === 'movie' ? 'movie' : 'series'
  return <Link to={itemPath(kind, item.id, item.library_id)}>{item.title}</Link>
}

function added(entry: ActivityEntry, p: Payload): ReactNode {
  const count = Array.isArray(p.items) ? p.items.length : 0
  if (entry.item?.kind === 'movie') {
    return (
      <>
        <ItemLink entry={entry} /> was added
      </>
    )
  }
  if (p.new_series) {
    return (
      <>
        <ItemLink entry={entry} /> was added with {count} {count === 1 ? 'episode' : 'episodes'}
      </>
    )
  }
  return (
    <>
      {count === 1 ? 'A new episode' : `${count} new episodes`} of <ItemLink entry={entry} />
    </>
  )
}

/** `at 23 min of 45`, from a session's last position. */
function stoppedAt(p: Payload) {
  const end = typeof p.position_end === 'number' ? p.position_end : null
  if (end == null) {
    return null
  }
  const total = typeof p.duration_seconds === 'number' ? p.duration_seconds : null
  const minutes = Math.round(end / 60)
  return total ? ` at ${minutes} of ${Math.round(total / 60)} min` : ` at ${minutes} min`
}

/** Approved, declined, turned off or back on. */
function statusChange(who: string, p: Payload) {
  const name = String(p.name ?? 'someone')
  if (p.status === 'active') {
    if (p.via) {
      return `${name} was let in by their ${String(p.via)} groups`
    }
    return p.from === 'pending' ? `${who} let ${name} in` : `${who} turned ${name} back on`
  }
  return p.from === 'pending' ? `${who} declined ${name}` : `${who} turned ${name} off`
}

function userChange(p: Payload) {
  const fields = Array.isArray(p.fields) ? (p.fields as string[]) : []
  const name = String(p.name ?? 'someone')
  const parts = []
  if (fields.includes('is_admin')) {
    parts.push(p.is_admin ? `made ${name} an admin` : `made ${name} no longer an admin`)
  }
  if (fields.includes('name')) {
    parts.push(`renamed an account to ${name}`)
  }
  if (fields.includes('password')) {
    parts.push(`reset ${name}'s password`)
  }
  return parts.join(', ')
}

function describe(entry: ActivityEntry, me: number): ReactNode {
  const p = (entry.payload ?? {}) as Payload
  const actor = entry.actor?.id === me ? 'You' : (entry.actor?.name ?? null)
  const who = actor ?? 'An API key'
  switch (entry.kind) {
    case 'items_added':
      return added(entry, p)
    case 'watched':
      return (
        <>
          {actor ?? 'Someone'} watched <ItemLink entry={entry} />
        </>
      )
    case 'stopped_watching':
      return (
        <>
          {actor ?? 'Someone'} stopped watching <ItemLink entry={entry} />
          {stoppedAt(p)}
        </>
      )
    case 'user_created':
      if (p.via) {
        const waiting = p.pending ? ' and is waiting to be let in' : ''
        return `${String(p.name)} signed in with ${String(p.via)} for the first time${waiting}`
      }
      return `${who} added the account ${String(p.name)}${p.is_admin ? ' (admin)' : ''}`
    case 'user_changed':
      if (p.kept_admin) {
        return `${String(p.name)} left ${String(p.via)}'s admin group but stays an admin: they're the only one`
      }
      return `${p.via ? String(p.via) : who} ${userChange(p)}`
    case 'user_status':
      return statusChange(who, p)
    case 'login_linked':
      return `${who} linked a ${String(p.via)} account to ${String(p.name)}`
    case 'login_unlinked':
      return `${who} unlinked ${String(p.name)}'s ${String(p.via)} account`
    case 'user_removed':
      return `${who} removed the account ${String(p.name)}`
    case 'login': {
      const via = p.via && p.via !== 'password' ? ` with ${String(p.via)}` : ''
      return `${entry.subject?.id === me ? 'You' : (entry.subject?.name ?? 'Someone')} signed in${via}`
    }
    case 'login_failed':
      if (p.via) {
        return `${String(p.via)} sign-in refused for “${String(p.name)}”: ${String(p.reason)}`
      }
      return `Failed sign-in as “${String(p.name)}”`
    case 'config_changed': {
      const fields = Array.isArray(p.fields) && p.fields.length ? ` (${p.fields.join(', ')})` : ''
      const target = TARGETS[String(p.target)] ?? String(p.target)
      const name = p.name ? ` ${String(p.name)}` : ''
      return `${who} ${ACTIONS[String(p.action)] ?? p.action} ${target}${name}${fields}`
    }
    case 'task_finished':
    case 'task_failed': {
      const verb = entry.kind === 'task_failed' ? 'failed' : 'finished'
      const error = p.error ? `: ${String(p.error)}` : ''
      const kind = TASK_KINDS[String(p.task_kind)] ?? `${String(p.task_kind)} task`
      const to = p.library_id ? `/manage/libraries/${String(p.library_id)}` : '/manage/libraries'
      return (
        <>
          <Link to={to}>
            {kind} {String(p.task_id)}
          </Link>{' '}
          {verb}
          {error}
        </>
      )
    }
    default:
      return entry.kind
  }
}

/** One entry: what happened, and when. */
export function ActivityLine({ entry }: { entry: ActivityEntry }) {
  const me = useUser()
  return (
    <li className={styles.entry} data-kind={entry.kind}>
      <span className={styles.text}>
        {describe(entry, me.id)}
        {entry.audience === 'subject' && (
          <span className={styles.private} title="Incognito: only they and admins see this">
            {' '}
            · incognito
          </span>
        )}
      </span>
      <time
        className={styles.time}
        dateTime={new Date(entry.at * 1000).toISOString()}
        title={dayTimeLabel(entry.at)}
      >
        {ago(entry.at)}
      </time>
    </li>
  )
}

export function ActivityList({ entries, empty }: { entries: ActivityEntry[]; empty: string }) {
  if (entries.length === 0) {
    return <p className={styles.empty}>{empty}</p>
  }
  return (
    <ol className={styles.list}>
      {entries.map((e) => (
        <ActivityLine key={e.id} entry={e} />
      ))}
    </ol>
  )
}
