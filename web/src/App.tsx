import { useEffect, useState } from 'react'
import { api, type Health } from './api/client'

export default function App() {
  const [health, setHealth] = useState<Health | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.health().then(setHealth, (e: Error) => setError(e.message))
  }, [])

  return (
    <main style={{ padding: 40 }}>
      <h1 style={{ fontSize: 30, fontWeight: 800, letterSpacing: '-0.02em', margin: 0 }}>
        Media Cabinet
      </h1>
      <p style={{ color: 'var(--text-muted)' }}>
        {error
          ? `Server unreachable: ${error}`
          : health
            ? `Server ${health.status} · v${health.version}`
            : 'Connecting…'}
      </p>
    </main>
  )
}
