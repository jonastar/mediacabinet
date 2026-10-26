import { useState, type FormEvent } from 'react'
import { api } from '../api/client'
import { LogoIcon } from '../components/Icons'
import { signedIn } from '../lib/session'
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

/** Sign in, or on a fresh install, create the admin account. */
export function AuthPage({ setup }: { setup: boolean }) {
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [again, setAgain] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

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
    <main className={styles.page}>
      <form className={styles.card} onSubmit={submit}>
        <span className={styles.brand}>
          <LogoIcon />
          Media Cabinet
        </span>
        <h1>{setup ? 'Create your account' : 'Sign in'}</h1>
        {setup && (
          <p className={styles.lead}>
            You'll be the admin. You can add accounts for everyone else afterwards.
          </p>
        )}
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
        {error && (
          <p className={styles.error} role="alert">
            {error}
          </p>
        )}
        <button type="submit" className={styles.submit} disabled={busy}>
          {setup ? 'Create account' : 'Sign in'}
        </button>
      </form>
    </main>
  )
}
