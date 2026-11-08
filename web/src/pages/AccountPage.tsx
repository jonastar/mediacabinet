import { useState, type FormEvent } from 'react'
import { api } from '../api/client'
import { useApi } from '../api/useApi'
import { Loading } from '../components/Status'
import { Link } from '../components/Link'
import { ago } from '../lib/format'
import { signedIn, signOut, useUser } from '../lib/session'
import styles from './Settings.module.css'

/** What linking an account left in the URL; read once, then removed. */
function takeLinkResult(): { ok: boolean; text: string } | null {
  const params = new URLSearchParams(window.location.search)
  const error = params.get('sign_in_error')
  const linked = params.get('linked')
  if (!error && !linked) {
    return null
  }
  window.history.replaceState(null, '', window.location.pathname)
  return error ? { ok: false, text: error } : { ok: true, text: 'Linked.' }
}

export function AccountPage() {
  const user = useUser()
  return user.is_guest ? <GuestAccount /> : <OwnAccount />
}

/** A guest has nothing to change; they can pair a TV and sign out. */
function GuestAccount() {
  const signOutGuest = () => {
    if (window.confirm('Sign out? What you watched and saved is forgotten.')) {
      signOut()
    }
  }
  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <div className={styles.heading}>
          <h1>Account</h1>
          <p className={styles.muted}>
            You're a guest. What you watch, your resume points and your playlists are your own,
            hidden from other guests, and forgotten when you sign out.
          </p>
        </div>
        <section className={styles.panel} aria-labelledby="devices">
          <h2 id="devices">Devices</h2>
          <p className={styles.muted}>
            If a TV or app is showing a code, <Link to="/pair">enter it here</Link> to sign it in as
            this guest.
          </p>
        </section>
        <section className={styles.panel} aria-labelledby="leave">
          <h2 id="leave">Sign out</h2>
          <p className={styles.muted}>Ends this guest visit. Signing in again starts afresh.</p>
          <div className={styles.row}>
            <button type="button" className={styles.button} onClick={signOutGuest}>
              Sign out
            </button>
          </div>
        </section>
      </div>
    </div>
  )
}

/** Your own name, email, password, linked accounts, devices and privacy. */
function OwnAccount() {
  const user = useUser()
  const [name, setName] = useState(user.name)
  const [email, setEmail] = useState(user.email ?? '')
  const [current, setCurrent] = useState('')
  const [password, setPassword] = useState('')
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(takeLinkResult)
  const signIn = useApi('my-sign-in', api.mySignIn)
  const hasPassword = signIn.data?.has_password ?? true
  const devices = useApi('my-devices', api.myDevices)

  const save = async (e: FormEvent, changes: Parameters<typeof api.updateMe>[0], done: string) => {
    e.preventDefault()
    setMessage(null)
    try {
      signedIn(await api.updateMe(changes))
      setMessage({ ok: true, text: done })
      setCurrent('')
      setPassword('')
      signIn.reload()
    } catch (err) {
      setMessage({ ok: false, text: (err as Error).message })
    }
  }

  const unlink = async (id: number, provider: string) => {
    if (!window.confirm(`Unlink your ${provider} account? You won't be able to sign in with it.`)) {
      return
    }
    setMessage(null)
    try {
      await api.unlinkMine(id)
      signIn.reload()
    } catch (err) {
      setMessage({ ok: false, text: (err as Error).message })
    }
  }

  const signOutDevice = async (id: number, device: string) => {
    if (!window.confirm(`Sign out ${device}? It'll need signing in again to use it.`)) {
      return
    }
    setMessage(null)
    try {
      await api.signOutDevice(id)
      devices.reload()
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
          onSubmit={(e) => save(e, { email }, email ? 'Email saved.' : 'Email removed.')}
          aria-labelledby="email"
        >
          <h2 id="email">Email</h2>
          <p className={styles.muted}>
            Signing in with another account that has this address offers to link it to this one.
          </p>
          <div className={styles.row}>
            <input
              className={styles.input}
              aria-label="Email"
              type="email"
              autoComplete="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
            <button
              type="submit"
              className={`${styles.button} ${styles.primary}`}
              disabled={email === (user.email ?? '')}
            >
              Save
            </button>
          </div>
        </form>
        <section className={styles.panel} aria-labelledby="linked">
          <h2 id="linked">Sign-in</h2>
          {!signIn.data && <Loading />}
          {signIn.data && (
            <>
              <p className={styles.muted}>
                {signIn.data.providers.length
                  ? 'Link an account you have elsewhere to sign in with it.'
                  : 'An admin can set up signing in with other accounts.'}
              </p>
              {signIn.data.providers.length > 0 && (
                <ul className={styles.list}>
                  {signIn.data.providers.map((p) => {
                    const login = signIn.data?.logins.find((l) => l.provider_key === p.key)
                    return (
                      <li key={p.key} className={styles.item}>
                        <span className={styles.itemText}>
                          <b>{p.name}</b>
                          <span className={styles.muted}>
                            {login
                              ? [
                                  login.email,
                                  `linked ${ago(login.linked_at)}`,
                                  login.last_used_at && `used ${ago(login.last_used_at)}`,
                                ]
                                  .filter(Boolean)
                                  .join(' · ')
                              : 'Not linked'}
                          </span>
                        </span>
                        {login ? (
                          <button
                            type="button"
                            className={styles.button}
                            onClick={() => unlink(login.id, p.name)}
                          >
                            Unlink
                          </button>
                        ) : (
                          <a
                            className={styles.button}
                            href={api.signInWithUrl(p.key, { link: true })}
                          >
                            Link
                          </a>
                        )}
                      </li>
                    )
                  })}
                </ul>
              )}
            </>
          )}
        </section>
        <form
          className={styles.panel}
          onSubmit={(e) =>
            save(
              e,
              hasPassword ? { password, current_password: current } : { password },
              hasPassword
                ? 'Password changed. Other devices are signed out.'
                : 'Password set. You can sign in with it as well.',
            )
          }
          aria-labelledby="password"
        >
          <h2 id="password">Password</h2>
          <p className={styles.muted}>
            {hasPassword
              ? 'Changing it signs you out on other devices.'
              : 'You sign in with another account. A password lets you sign in without it too.'}
          </p>
          <div className={styles.row}>
            {hasPassword && (
              <input
                className={styles.input}
                aria-label="Current password"
                placeholder="Current password"
                type="password"
                autoComplete="current-password"
                value={current}
                onChange={(e) => setCurrent(e.target.value)}
              />
            )}
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
              disabled={(hasPassword && !current) || !password}
            >
              {hasPassword ? 'Change password' : 'Set password'}
            </button>
          </div>
        </form>
        <section className={styles.panel} aria-labelledby="devices">
          <h2 id="devices">Devices</h2>
          {!devices.data && <Loading />}
          {devices.data && (
            <>
              <p className={styles.muted}>
                {devices.data.length
                  ? 'TVs and apps signed in to your account.'
                  : 'No TVs or apps are signed in.'}{' '}
                If one is showing a code, <Link to="/pair">enter it here</Link>.
              </p>
              {devices.data.length > 0 && (
                <ul className={styles.list}>
                  {devices.data.map((d) => (
                    <li key={d.id} className={styles.item}>
                      <span className={styles.itemText}>
                        <b>{d.name}</b>
                        <span className={styles.muted}>
                          signed in {ago(d.signed_in_at)} · used {ago(d.last_seen_at)}
                        </span>
                      </span>
                      <button
                        type="button"
                        className={styles.button}
                        onClick={() => signOutDevice(d.id, d.name)}
                      >
                        Sign out
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </>
          )}
        </section>
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
