import { useEffect, useState } from 'react'
import {
  api,
  type CheckReport,
  type Note,
  type ProviderInput,
  type TestReport,
} from '../../api/client'
import settings from '../Settings.module.css'
import styles from './Users.module.css'

/** The sign-in window's last page posts the report's token here (see `test_done`). */
const CHANNEL = 'mc-sign-in-test'

const MARKS: Record<Note['level'], string> = {
  good: '✓',
  info: 'i',
  warning: '!',
  problem: '✕',
}

/** Text with `backticked` parts in code. */
function Ticked({ text }: { text: string }) {
  return <>{text.split('`').map((part, i) => (i % 2 ? <code key={i}>{part}</code> : part))}</>
}

function Notes({ notes }: { notes: Note[] }) {
  return (
    <ul className={styles.notes}>
      {notes.map((n, i) => (
        <li key={i} className={styles[n.level]}>
          <span className={styles.mark} aria-label={n.level}>
            {MARKS[n.level]}
          </span>
          <span>
            <Ticked text={n.text} />
          </span>
        </li>
      ))}
    </ul>
  )
}

function groupsText(groups: string[] | null | undefined) {
  if (groups == null) {
    return 'No groups claim'
  }
  return groups.length ? groups.join(', ') : 'None'
}

/** What came back from a test sign-in. */
export function TestReportView({ report }: { report: TestReport }) {
  return (
    <div className={styles.report}>
      {report.outcome && <p className={styles.outcome}>{report.outcome}</p>}
      {report.subject && (
        <dl className={styles.facts}>
          <dt>Account</dt>
          <dd>
            <code>{report.subject}</code>
          </dd>
          <dt>Name</dt>
          <dd>{report.name}</dd>
          <dt>Email</dt>
          <dd>
            {report.email
              ? `${report.email} (${report.email_verified ? 'verified' : 'not verified'})`
              : 'None'}
          </dd>
          <dt>Groups</dt>
          <dd>{groupsText(report.groups)}</dd>
          <dt>Scopes</dt>
          <dd>
            <code>{report.granted_scopes ?? (report.scopes || 'none')}</code>{' '}
            {report.granted_scopes ? '(granted)' : '(asked for)'}
          </dd>
          <dt>Claims</dt>
          <dd>
            <code>{report.claims.join(' ')}</code>
          </dd>
        </dl>
      )}
      <Notes notes={report.notes} />
    </div>
  )
}

/**
 * Checks a provider's settings, saved or not, and tries signing in with them in a new window.
 * Nobody is signed in and nothing is saved.
 */
export function ProviderTest({
  existing,
  input,
  ready,
  onIssuer,
}: {
  /** The provider being edited, for its saved secret. */
  existing: string | null
  input: () => ProviderInput
  ready: boolean
  /** The issuer as the provider names itself, when it differs. */
  onIssuer: (issuer: string) => void
}) {
  const [checking, setChecking] = useState(false)
  const [check, setCheck] = useState<CheckReport | null>(null)
  const [trying, setTrying] = useState(false)
  const [report, setReport] = useState<TestReport | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!trying) {
      return
    }
    const channel = new BroadcastChannel(CHANNEL)
    channel.onmessage = (e: MessageEvent<{ token?: unknown }>) => {
      const token = e.data?.token
      if (typeof token !== 'string') {
        return
      }
      channel.postMessage({ received: token })
      setTrying(false)
      api.testReport(token).then(setReport, (err: Error) => setError(err.message))
    }
    return () => channel.close()
  }, [trying])

  const runCheck = async () => {
    setChecking(true)
    setError(null)
    setReport(null)
    try {
      const found = await api.checkProvider(existing, input())
      setCheck(found)
      if (found.issuer && found.issuer !== input().issuer) {
        onIssuer(found.issuer)
      }
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setChecking(false)
    }
  }

  const trySignIn = async () => {
    setError(null)
    setCheck(null)
    setReport(null)
    // Opened now, while the click still counts, or the browser blocks it.
    const popup = window.open('', 'mc-sign-in-test', 'popup,width=520,height=720')
    if (!popup) {
      setError('Allow pop-ups for this site to try signing in.')
      return
    }
    try {
      const { url } = await api.tryProvider(existing, input())
      popup.location.href = url
      setTrying(true)
    } catch (e) {
      popup.close()
      setError((e as Error).message)
    }
  }

  return (
    <div className={styles.field}>
      <span className={styles.label}>Test</span>
      <span className={styles.hint}>
        Check asks the provider about these settings. Trying signs in with it in a new window and
        shows what comes back; nobody is signed in and nothing is saved. If the provider complains
        about the redirect URI, add the redirect URL above to it.
      </span>
      <span className={settings.row}>
        <button
          type="button"
          className={settings.button}
          disabled={!ready || checking}
          onClick={runCheck}
        >
          {checking ? 'Checking…' : 'Check settings'}
        </button>
        <button
          type="button"
          className={settings.button}
          disabled={!ready || trying}
          onClick={trySignIn}
        >
          Try signing in
        </button>
        {trying && (
          <span className={styles.hint}>
            Waiting for the sign-in window…{' '}
            <button type="button" className={styles.linkButton} onClick={() => setTrying(false)}>
              Stop waiting
            </button>
          </span>
        )}
      </span>
      {error && <p className={settings.warning}>{error}</p>}
      {check && (
        <div className={styles.report}>
          <Notes notes={check.notes} />
          {check.scopes && (
            <p className={styles.hint}>
              Asks for <code>{check.scopes}</code>.
            </p>
          )}
        </div>
      )}
      {report && <TestReportView report={report} />}
    </div>
  )
}
