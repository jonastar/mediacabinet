import { useCallback, useEffect, useState } from 'preact/hooks'
import { api, type PairingStarted } from '../api'
import { useInitialFocus } from '../focus'
import { platform } from '../platform'

/** Shows a code to approve on the web, and waits for it. */
export function Pair({
  server,
  onPaired,
  onChangeServer,
}: {
  server: string
  onPaired: (token: string) => void
  onChangeServer: () => void
}) {
  const [pairing, setPairing] = useState<PairingStarted | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const [guests, setGuests] = useState(false)
  const restart = useCallback(() => setAttempt((n) => n + 1), [])
  useInitialFocus(null)

  useEffect(() => {
    let current = true
    setPairing(null)
    setProblem(null)
    api.startPairing(platform.deviceName).then(
      (started) => {
        if (current) {
          setPairing(started)
        }
      },
      (err: Error) => {
        if (current) {
          setProblem(err.message)
        }
      },
    )
    return () => {
      current = false
    }
  }, [attempt])

  useEffect(() => {
    let current = true
    api.serverInfo(server).then(
      (info) => {
        if (current) {
          setGuests(info.guest_access)
        }
      },
      () => undefined,
    )
    return () => {
      current = false
    }
  }, [server])

  const continueAsGuest = () => {
    setProblem(null)
    api.guest(platform.deviceName).then(
      (session) => onPaired(session.token),
      (err: Error) => setProblem(err.message),
    )
  }

  useEffect(() => {
    if (!pairing) {
      return
    }
    const timer = window.setInterval(() => {
      api.pollPairing(pairing.secret).then(
        ({ state, session }) => {
          if (state === 'approved' && session) {
            window.clearInterval(timer)
            onPaired(session.token)
          } else if (state === 'declined') {
            window.clearInterval(timer)
            setPairing(null)
            setProblem('That was turned away.')
          } else if (state === 'expired') {
            // Took too long: a fresh code, without anyone having to ask.
            window.clearInterval(timer)
            restart()
          }
        },
        // The network comes and goes; the next poll tries again.
        () => undefined,
      )
    }, pairing.poll_every_seconds * 1000)
    return () => window.clearInterval(timer)
  }, [pairing, onPaired, restart])

  return (
    <main className="center">
      <h1>Sign in</h1>
      <p className="muted">On your phone or computer, go to</p>
      <p className="address">{server}/pair</p>
      <p className="muted">and enter this code:</p>
      <p className="code">{pairing ? pairing.code : '…'}</p>
      {problem && (
        <p className="problem" role="alert">
          {problem}
        </p>
      )}
      <div className="actions">
        {problem && <button onClick={restart}>Try again</button>}
        {guests && <button onClick={continueAsGuest}>Continue as guest</button>}
        <button onClick={onChangeServer}>Change server</button>
      </div>
    </main>
  )
}
