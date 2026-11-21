import { useState } from 'preact/hooks'
import { api, normaliseAddress } from '../api'
import { useInitialFocus } from '../focus'
import { APP_PLATFORM, APP_VERSION, MIN_SERVER_VERSION, meetsMinimum } from '../version'

/** Asks where the server is. */
export function Connect({ onConnected }: { onConnected: (address: string) => void }) {
  const [typed, setTyped] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  useInitialFocus(null)

  const connect = async (e: Event) => {
    e.preventDefault()
    const address = normaliseAddress(typed)
    setBusy(true)
    setError(null)
    try {
      const info = await api.serverInfo(address)
      const minApp = APP_PLATFORM && info.min_app_versions[APP_PLATFORM]
      if (!meetsMinimum(info.version, MIN_SERVER_VERSION)) {
        setError(
          `${info.name} runs Media Cabinet ${info.version}. Update it to ${MIN_SERVER_VERSION} or later to connect.`,
        )
      } else if (APP_VERSION && minApp && !meetsMinimum(APP_VERSION, minApp)) {
        setError(`${info.name} needs version ${minApp} of this app or later.`)
      } else {
        onConnected(address)
      }
    } catch {
      setError(`Can't reach a Media Cabinet server at ${address}.`)
    }
    setBusy(false)
  }

  return (
    <main className="center above-keyboard">
      <h1>Connect to your server</h1>
      <p className="muted">The address you open Media Cabinet at, such as 192.168.1.20:8420.</p>
      <form onSubmit={connect}>
        <input
          aria-label="Server address"
          value={typed}
          onInput={(e) => setTyped(e.currentTarget.value)}
          autoComplete="off"
          spellcheck={false}
        />
        <div className="actions">
          <button type="submit" disabled={busy || !typed.trim()}>
            {busy ? 'Connecting…' : 'Connect'}
          </button>
        </div>
      </form>
      {error && (
        <p className="problem" role="alert">
          {error}
        </p>
      )}
    </main>
  )
}
