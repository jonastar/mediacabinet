import { useEffect, useState, type FormEvent, type ReactNode } from 'react'
import { api, type Confirmation } from '../api/client'
import { Brand } from '../components/Brand'
import {
  checkSession,
  signedIn,
  signedOutOnPurpose,
  signOut,
  type SignInOptions,
} from '../lib/session'
import styles from './AuthPage.module.css'

function Field({
  label,
  type = 'text',
  value,
  onChange,
  autoComplete,
}: {
  label: string
  type?: string
  value: string
  onChange: (value: string) => void
  autoComplete: string
}) {
  return (
    <label className={styles.field}>
      {label}
      <input
        type={type}
        value={value}
        autoComplete={autoComplete}
        required
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  )
}

function Card({ children, onSubmit }: { children: ReactNode; onSubmit?: (e: FormEvent) => void }) {
  return (
    <main className={styles.page}>
      <form className={styles.card} onSubmit={onSubmit ?? ((e) => e.preventDefault())}>
        <span className={styles.brand}>
          <Brand />
        </span>
        {children}
      </form>
    </main>
  )
}

/**
 * What a provider's round trip left in the URL: an error, or `confirm` for
 * "Is this you?". Read once, then removed so a reload doesn't repeat it.
 */
function takeSignInParams() {
  const params = new URLSearchParams(window.location.search)
  const error = params.get('sign_in_error')
  const confirm = params.get('sign_in') === 'confirm'
  if (error || params.has('sign_in')) {
    params.delete('sign_in_error')
    params.delete('sign_in')
    const rest = params.toString()
    window.history.replaceState(null, '', window.location.pathname + (rest ? `?${rest}` : ''))
  }
  return { error, confirm }
}

/** Sign in, or on a fresh install, create the admin account. */
export function AuthPage({ setup, options }: { setup: boolean; options?: SignInOptions }) {
  const [params] = useState(takeSignInParams)
  const [confirming, setConfirming] = useState(params.confirm)
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [again, setAgain] = useState('')
  const [error, setError] = useState<string | null>(params.error)
  const [busy, setBusy] = useState(false)
  const providers = setup ? [] : (options?.providers ?? [])
  const passwordFirst = setup || !providers.length || (options?.passwordSignIn ?? true)
  const [showPassword, setShowPassword] = useState(passwordFirst)
  // Password sign-in hidden and one provider: go straight there, unless
  // something went wrong or they just signed out.
  const straightTo =
    !passwordFirst &&
    providers.length === 1 &&
    !params.error &&
    !confirming &&
    !signedOutOnPurpose()
      ? providers[0]
      : null

  useEffect(() => {
    if (straightTo) {
      window.location.assign(api.signInWithUrl(straightTo.key))
    }
  }, [straightTo])

  if (confirming) {
    return <Confirm onCancel={() => setConfirming(false)} />
  }
  if (straightTo) {
    return (
      <Card>
        <p className={styles.lead}>Taking you to {straightTo.name}…</p>
      </Card>
    )
  }

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    if (setup && password !== again) {
      setError("The passwords don't match.")
      return
    }
    setBusy(true)
    setError(null)
    try {
      signedIn(setup ? await api.setup(name, password) : await api.login(name, password))
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <Card onSubmit={submit}>
      <h1>{setup ? 'Create your account' : 'Sign in'}</h1>
      {setup && (
        <p className={styles.lead}>
          You'll be the admin. You can add accounts for everyone else afterwards.
        </p>
      )}
      {providers.length > 0 && (
        <div className={styles.providers}>
          {providers.map((p) => (
            <a key={p.key} className={styles.provider} href={api.signInWithUrl(p.key)}>
              Sign in with {p.name}
            </a>
          ))}
        </div>
      )}
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      {providers.length > 0 && showPassword && <p className={styles.or}>or</p>}
      {!showPassword && (
        <button type="button" className={styles.quiet} onClick={() => setShowPassword(true)}>
          Use a password
        </button>
      )}
      {showPassword && (
        <>
          <Field label="Name" value={name} onChange={setName} autoComplete="username" />
          <Field
            label="Password"
            type="password"
            value={password}
            onChange={setPassword}
            autoComplete={setup ? 'new-password' : 'current-password'}
          />
          {setup && (
            <Field
              label="Password again"
              type="password"
              value={again}
              onChange={setAgain}
              autoComplete="new-password"
            />
          )}
          <button type="submit" className={styles.submit} disabled={busy}>
            {setup ? 'Create account' : 'Sign in'}
          </button>
        </>
      )}
    </Card>
  )
}

/** "Is this you?": the provider's verified email belongs to an account here. */
function Confirm({ onCancel }: { onCancel: () => void }) {
  const [info, setInfo] = useState<Confirmation | null>(null)
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    api.confirmation().then(setInfo, (e: Error) => setError(e.message))
  }, [])

  const act = async (call: () => ReturnType<typeof api.confirm>) => {
    setBusy(true)
    setError(null)
    try {
      signedIn(await call())
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(false)
    }
  }

  if (!info) {
    return (
      <Card>
        <h1>Is this you?</h1>
        {error ? (
          <>
            <p className={styles.error} role="alert">
              {error}
            </p>
            <button type="button" className={styles.submit} onClick={onCancel}>
              Back to sign in
            </button>
          </>
        ) : (
          <p className={styles.lead}>Checking…</p>
        )}
      </Card>
    )
  }

  return (
    <Card
      onSubmit={(e) => {
        e.preventDefault()
        act(() => api.confirm(password))
      }}
    >
      <h1>Is this you?</h1>
      <p className={styles.lead}>
        The email address of your {info.provider} account belongs to <b>{info.account}</b> here.
        Prove it's you to link the two; after that, {info.provider} signs you straight in.
      </p>
      {info.has_password && (
        <>
          <Field
            label={`${info.account}'s password`}
            type="password"
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
          />
          <button type="submit" className={styles.submit} disabled={busy || !password}>
            Link and sign in
          </button>
        </>
      )}
      {info.other_providers.length > 0 && (
        <div className={styles.providers}>
          {info.other_providers.map((p) => (
            <a
              key={p.key}
              className={styles.provider}
              href={api.signInWithUrl(p.key, { thenConfirm: true })}
            >
              Prove it with {p.name}
            </a>
          ))}
        </div>
      )}
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <button
        type="button"
        className={styles.quiet}
        disabled={busy}
        onClick={() => act(api.declineConfirm)}
      >
        No, make me a new account
      </button>
    </Card>
  )
}

/** Signed in with another account, waiting for an admin to let them in. */
export function PendingPage({ name }: { name: string }) {
  useEffect(() => {
    // Let them in as soon as they come back to the tab after being approved.
    const recheck = () => {
      if (document.visibilityState === 'visible') {
        checkSession()
      }
    }
    const timer = window.setInterval(recheck, 30_000)
    window.addEventListener('focus', recheck)
    return () => {
      window.clearInterval(timer)
      window.removeEventListener('focus', recheck)
    }
  }, [])
  return (
    <Card>
      <h1>Almost there, {name}</h1>
      <p className={styles.lead}>
        An admin needs to let you in. This page carries on by itself once they have.
      </p>
      <button type="button" className={styles.quiet} onClick={signOut}>
        Sign out
      </button>
    </Card>
  )
}
