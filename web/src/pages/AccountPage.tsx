import { useState, type FormEvent } from 'react'
import { api } from '../api/client'
import { signedIn, useUser } from '../lib/session'
import styles from './Settings.module.css'

/** Your own name, password and privacy. */
export function AccountPage() {
  const user = useUser()
  const [name, setName] = useState(user.name)
  const [current, setCurrent] = useState('')
  const [password, setPassword] = useState('')
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null)

  const save = async (e: FormEvent, changes: Parameters<typeof api.updateMe>[0], done: string) => {
    e.preventDefault()
    setMessage(null)
    try {
      signedIn(await api.updateMe(changes))
      setMessage({ ok: true, text: done })
      setCurrent('')
      setPassword('')
    } catch (err) {
      setMessage({ ok: false, text: (err as Error).message })
    }
  }

  const setIncognito = async (on: boolean) => {
    setMessage(null)
    try {
      signedIn(await api.updateMe({ incognito: on }))
    } catch (err) {
      setMessage({ ok: false, text: (err as Error).message })
    }
  }

  const clearWatchActivity = async () => {
    if (!window.confirm('Clear what you watched from the activity log? Resume points stay.')) {
      return
    }
    setMessage(null)
    try {
      const { removed } = await api.clearMyWatchActivity()
      setMessage({
        ok: true,
        text: removed === 1 ? 'Cleared 1 entry.' : `Cleared ${removed} entries.`,
      })
    } catch (err) {
      setMessage({ ok: false, text: (err as Error).message })
    }
  }

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <div className={styles.heading}>
          <h1>Account</h1>
          <p className={styles.muted}>
            {user.is_admin ? 'Admin' : 'Viewer'}. Your watch history and playlists are your own.
          </p>
        </div>
        {message && (
          <p className={message.ok ? styles.muted : styles.warning} role="status">
            {message.text}
          </p>
        )}
        <form
          className={styles.panel}
          onSubmit={(e) => save(e, { name }, 'Name saved.')}
          aria-labelledby="name"
        >
          <h2 id="name">Name</h2>
          <div className={styles.row}>
            <input
              className={styles.input}
              aria-label="Name"
              autoComplete="username"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <button
              type="submit"
              className={`${styles.button} ${styles.primary}`}
              disabled={!name.trim() || name === user.name}
            >
              Save
            </button>
          </div>
        </form>
        <form
          className={styles.panel}
          onSubmit={(e) =>
            save(
              e,
              { password, current_password: current },
              'Password changed. Other devices are signed out.',
            )
          }
          aria-labelledby="password"
        >
          <h2 id="password">Password</h2>
          <p className={styles.muted}>Changing it signs you out on other devices.</p>
          <div className={styles.row}>
            <input
              className={styles.input}
              aria-label="Current password"
              placeholder="Current password"
              type="password"
              autoComplete="current-password"
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
            />
            <input
              className={styles.input}
              aria-label="New password"
              placeholder="New password (8+ characters)"
              type="password"
              autoComplete="new-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button
              type="submit"
              className={`${styles.button} ${styles.primary}`}
              disabled={!current || !password}
            >
              Change password
            </button>
          </div>
        </form>
        <section className={styles.panel} aria-labelledby="privacy">
          <h2 id="privacy">Privacy</h2>
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={user.incognito}
              onChange={(e) => setIncognito(e.target.checked)}
            />
            Incognito
          </label>
          <p className={styles.muted}>
            Hides what you watch from other users from now on. Admins still see it. What you watched
            before keeps the setting it had then.
          </p>
          <div className={styles.row}>
            <button type="button" className={styles.button} onClick={clearWatchActivity}>
              Clear my watch activity
            </button>
          </div>
        </section>
      </div>
    </div>
  )
}
