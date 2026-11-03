import { useState } from 'react'
import { api, type NewApiKey, type NewWebhook, type Webhook } from '../api/client'
import { useApi } from '../api/useApi'
import { Failed, Loading } from '../components/Status'
import { ago } from '../lib/format'
import styles from './Settings.module.css'

/** A secret shown once, with a copy button. */
function Secret({ label, value, onDone }: { label: string; value: string; onDone: () => void }) {
  const [copied, setCopied] = useState(false)
  return (
    <div className={styles.secret} role="status">
      <p className={styles.muted}>{label} It won't be shown again.</p>
      <code className={styles.code}>{value}</code>
      <div className={styles.row}>
        <button
          type="button"
          className={styles.button}
          onClick={() => navigator.clipboard?.writeText(value).then(() => setCopied(true))}
        >
          {copied ? 'Copied' : 'Copy'}
        </button>
        <button type="button" className={styles.button} onClick={onDone}>
          Done
        </button>
      </div>
    </div>
  )
}

function ApiKeys() {
  const keys = useApi('api-keys', api.apiKeys)
  const [name, setName] = useState('')
  const [created, setCreated] = useState<NewApiKey | null>(null)
  if (keys.error) {
    return <Failed error={keys.error} onRetry={keys.reload} />
  }
  const create = async () => {
    setCreated(await api.createApiKey(name))
    setName('')
    keys.reload()
  }
  return (
    <section aria-labelledby="keys" className={styles.panel}>
      <h2 id="keys">API keys</h2>
      <p className={styles.muted}>
        For Sonarr, Radarr and other tools. Send it as an <code>X-Api-Key</code> header or{' '}
        <code>?apikey=</code>.
      </p>
      {created && (
        <Secret
          label={`Key for ${created.name}.`}
          value={created.secret}
          onDone={() => setCreated(null)}
        />
      )}
      {!keys.data && <Loading />}
      {keys.data && keys.data.length > 0 && (
        <ul className={styles.list}>
          {keys.data.map((k) => (
            <li key={k.id} className={styles.item}>
              <span className={styles.itemText}>
                <b>{k.name}</b>
                <span className={styles.muted}>
                  <code>{k.prefix}…</code> ·{' '}
                  {k.last_used_at ? `used ${ago(k.last_used_at)}` : 'never used'}
                </span>
              </span>
              <button
                type="button"
                className={`${styles.button} ${styles.danger}`}
                onClick={() =>
                  window.confirm(`Revoke the key for ${k.name}?`) &&
                  api.revokeApiKey(k.id).then(keys.reload)
                }
              >
                Revoke
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className={styles.row}>
        <input
          className={styles.input}
          aria-label="Key name"
          placeholder="What it's for, e.g. Sonarr"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button
          type="button"
          className={`${styles.button} ${styles.primary}`}
          disabled={!name.trim()}
          onClick={create}
        >
          Create key
        </button>
      </div>
    </section>
  )
}

const COMMON_EVENTS = [
  'item.wanted',
  'item.unwanted',
  'file.added',
  'file.missing',
  'file.upgraded',
  'link.created',
  'match.needs_review',
  'status.changed',
  'scan.finished',
]

function webhookState(w: Webhook) {
  if (!w.enabled) {
    return 'Paused'
  }
  if (w.failures > 0) {
    return `Failing (${w.failures}×): ${w.last_error ?? 'unknown error'} · ${w.pending} waiting`
  }
  const delivered = w.last_delivered_at
    ? `delivered ${ago(w.last_delivered_at)}`
    : 'nothing delivered yet'
  return `${delivered}${w.pending ? ` · ${w.pending} waiting` : ''}`
}

function Webhooks() {
  const hooks = useApi('webhooks', api.webhooks)
  const [draft, setDraft] = useState({ name: '', url: '', types: [] as string[] })
  const [created, setCreated] = useState<NewWebhook | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [tests, setTests] = useState<Record<number, string>>({})
  if (hooks.error) {
    return <Failed error={hooks.error} onRetry={hooks.reload} />
  }
  const create = async () => {
    setError(null)
    try {
      setCreated(
        await api.createWebhook({ name: draft.name, url: draft.url, event_types: draft.types }),
      )
      setDraft({ name: '', url: '', types: [] })
      hooks.reload()
    } catch (e) {
      setError((e as Error).message)
    }
  }
  const test = async (id: number) => {
    setTests((t) => ({ ...t, [id]: 'Sending…' }))
    const r = await api.testWebhook(id)
    setTests((t) => ({ ...t, [id]: r.ok ? 'Delivered' : `Failed: ${r.error}` }))
  }
  const toggle = (type: string) =>
    setDraft((d) => ({
      ...d,
      types: d.types.includes(type) ? d.types.filter((t) => t !== type) : [...d.types, type],
    }))

  return (
    <section aria-labelledby="hooks" className={styles.panel}>
      <h2 id="hooks">Webhooks</h2>
      <p className={styles.muted}>
        Events are posted in order as JSON, signed with <code>X-Cabinet-Signature: sha256=…</code>{' '}
        (HMAC of the body with the webhook's secret). Missed events are retried; nothing is skipped.
      </p>
      {created && (
        <Secret
          label={`Signing secret for ${created.url}.`}
          value={created.secret}
          onDone={() => setCreated(null)}
        />
      )}
      {!hooks.data && <Loading />}
      {hooks.data && hooks.data.length > 0 && (
        <ul className={styles.list}>
          {hooks.data.map((w) => (
            <li key={w.id} className={styles.item}>
              <span className={styles.itemText}>
                <b>{w.name || w.url}</b>
                <span className={styles.muted}>
                  <code>{w.url}</code> ·{' '}
                  {w.event_types.length ? w.event_types.join(', ') : 'all events'}
                </span>
                <span className={w.failures > 0 ? styles.warning : styles.muted}>
                  {webhookState(w)}
                </span>
                {tests[w.id] && <span className={styles.muted}>Test: {tests[w.id]}</span>}
              </span>
              <span className={styles.row}>
                <button type="button" className={styles.button} onClick={() => test(w.id)}>
                  Test
                </button>
                <button
                  type="button"
                  className={styles.button}
                  onClick={() =>
                    api.updateWebhook(w.id, { enabled: !w.enabled }).then(hooks.reload)
                  }
                >
                  {w.enabled ? 'Pause' : 'Resume'}
                </button>
                <button
                  type="button"
                  className={`${styles.button} ${styles.danger}`}
                  onClick={() =>
                    window.confirm(`Delete the webhook to ${w.url}?`) &&
                    api.deleteWebhook(w.id).then(hooks.reload)
                  }
                >
                  Delete
                </button>
              </span>
            </li>
          ))}
        </ul>
      )}
      <div className={styles.form}>
        <div className={styles.row}>
          <input
            className={styles.input}
            aria-label="Webhook name"
            placeholder="Name"
            value={draft.name}
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          />
          <input
            className={`${styles.input} ${styles.wide}`}
            aria-label="Webhook URL"
            placeholder="https://example.local/hooks/media-cabinet"
            value={draft.url}
            onChange={(e) => setDraft({ ...draft, url: e.target.value })}
          />
        </div>
        <fieldset className={styles.types}>
          <legend className={styles.muted}>Events (none ticked: all of them)</legend>
          {COMMON_EVENTS.map((t) => (
            <label key={t} className={styles.check}>
              <input type="checkbox" checked={draft.types.includes(t)} onChange={() => toggle(t)} />
              <code>{t}</code>
            </label>
          ))}
        </fieldset>
        {error && <p className={styles.warning}>{error}</p>}
        <div className={styles.row}>
          <button
            type="button"
            className={`${styles.button} ${styles.primary}`}
            disabled={!draft.url}
            onClick={create}
          >
            Add webhook
          </button>
        </div>
      </div>
    </section>
  )
}

function ArrSetup() {
  const origin = window.location.origin
  const rows = [
    ['Sonarr → Import Lists → Custom List', `${origin}/api/arr/sonarr-list?apikey=YOUR_KEY`],
    [
      'Radarr → Lists → Radarr List (or StevenLu Custom)',
      `${origin}/api/arr/radarr-list?apikey=YOUR_KEY`,
    ],
    ['Sonarr / Radarr → Connect → Webhook (POST)', `${origin}/api/arr/webhook?apikey=YOUR_KEY`],
  ]
  return (
    <section aria-labelledby="arr" className={styles.panel}>
      <h2 id="arr">Sonarr and Radarr</h2>
      <p className={styles.muted}>
        No plugins needed. The lists hand them what's wanted; the webhook tells Media Cabinet to
        rescan when they import, upgrade, rename or delete. Use an API key from above.
      </p>
      <dl className={styles.setup}>
        {rows.map(([where, url]) => (
          <div key={where}>
            <dt>{where}</dt>
            <dd>
              <code className={styles.code}>{url}</code>
            </dd>
          </div>
        ))}
      </dl>
    </section>
  )
}

export function SettingsPage() {
  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <div className={styles.heading}>
          <h1>Settings</h1>
          <p className={styles.muted}>
            Connections to other tools. Users and sign-in are under Users.
          </p>
        </div>
        <ApiKeys />
        <ArrSetup />
        <Webhooks />
      </div>
    </div>
  )
}
