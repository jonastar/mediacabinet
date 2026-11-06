import { useState } from 'preact/hooks'
import { api, normaliseAddress } from '../api'
import { useInitialFocus } from '../focus'

/** The app's version, against the oldest the server still works with. */
const APP_VERSION = '1.0.0'

function older(version: string, than: string) {
  const a = version.split('.').map(Number)
  const b = than.split('.').map(Number)
  for (let i = 0; i < 3; i++) {
    if ((a[i] || 0) !== (b[i] || 0)) {
      return (a[i] || 0) < (b[i] || 0)
    }
  }
  return false
}

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
      if (older(APP_VERSION, info.min_app_version)) {
        setError(`${info.name} needs a newer version of this app.`)
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
