import { useEffect, useState, type FormEvent } from 'react'
import { api } from '../api/client'
import { useQueryParam } from '../lib/router'
import { useUser } from '../lib/session'
import styles from './Settings.module.css'

type Step =
  | { name: 'enter' }
  | { name: 'confirm'; code: string; device: string }
  | { name: 'done'; device: string; approved: boolean }

/** Signs a TV or app in to your account, using the code it shows. */
export function PairPage() {
  const user = useUser()
  const fromLink = useQueryParam('code')
  const [code, setCode] = useState(fromLink ?? '')
  const [step, setStep] = useState<Step>({ name: 'enter' })
  const [error, setError] = useState<string | null>(null)

  const lookUp = async (typed: string) => {
    setError(null)
    try {
      const { device_name } = await api.pairingRequest(typed)
      setStep({ name: 'confirm', code: typed, device: device_name })
    } catch (err) {
      setError((err as Error).message)
    }
  }

  // A link already carries the code, so go straight to asking.
  useEffect(() => {
    if (!fromLink) {
      return
    }
    let current = true
    api.pairingRequest(fromLink).then(
      ({ device_name }) => {
        if (current) {
          setStep({ name: 'confirm', code: fromLink, device: device_name })
        }
      },
      (err: Error) => {
        if (current) {
          setCode(fromLink)
          setError(err.message)
        }
      },
    )
    return () => {
      current = false
    }
  }, [fromLink])

  const answer = async (typed: string, approve: boolean) => {
    setError(null)
    try {
      const { device_name } = await api.answerPairing(typed, approve)
      setStep({ name: 'done', device: device_name, approved: approve })
    } catch (err) {
      setStep({ name: 'enter' })
      setError((err as Error).message)
    }
  }

  const submit = (e: FormEvent) => {
    e.preventDefault()
    lookUp(code)
  }

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <div className={styles.heading}>
          <h1>Sign in a device</h1>
          <p className={styles.muted}>For a TV or an app showing a code.</p>
        </div>
        {error && (
          <p className={styles.warning} role="alert">
            {error}
          </p>
        )}
        {step.name === 'enter' && (
          <form className={styles.panel} onSubmit={submit} aria-labelledby="pair-code">
            <h2 id="pair-code">Code</h2>
            <div className={styles.row}>
              <input
                className={styles.input}
                aria-label="Code"
                placeholder="XXXX-XXXX"
                autoComplete="off"
                autoCapitalize="characters"
                spellCheck={false}
                value={code}
                onChange={(e) => setCode(e.target.value)}
              />
              <button
                type="submit"
                className={`${styles.button} ${styles.primary}`}
                disabled={!code.trim()}
              >
                Continue
              </button>
            </div>
          </form>
        )}
        {step.name === 'confirm' && (
          <section className={styles.panel} aria-labelledby="pair-confirm">
            <h2 id="pair-confirm">Sign in {step.device}?</h2>
            <p className={styles.muted}>
              It will be signed in as {user.name}, and can do what you can. Only continue if you can
              see this code on it: <b>{step.code}</b>. You can sign it out in Account.
            </p>
            <div className={styles.row}>
              <button
                type="button"
                className={`${styles.button} ${styles.primary}`}
                onClick={() => answer(step.code, true)}
              >
                Sign in
              </button>
              <button
                type="button"
                className={styles.button}
                onClick={() => answer(step.code, false)}
              >
                Not this one
              </button>
            </div>
          </section>
        )}
        {step.name === 'done' && (
          <section className={styles.panel} role="status">
            <h2>{step.approved ? 'All done' : 'Turned away'}</h2>
            <p className={styles.muted}>
              {step.approved
                ? `${step.device} is signing in as ${user.name}. You can go back to it now.`
                : `${step.device} wasn't signed in.`}
            </p>
          </section>
        )}
      </div>
    </div>
  )
}
