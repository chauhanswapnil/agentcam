import { useEffect, useState } from 'react'

type LogSource = { agent: string; path: string; exists: boolean }
type Health = { name: string; version: string; sources: LogSource[] }

export default function App() {
  const [health, setHealth] = useState<Health | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    fetch('/api/health')
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then(setHealth)
      .catch((e: Error) => setError(e.message))
  }, [])

  return (
    <main style={{ maxWidth: 640, margin: '0 auto', padding: '64px 24px' }}>
      <h1 style={{ fontSize: 20, fontWeight: 600, margin: 0 }}>
        agentcam <span className="mono" style={{ color: 'var(--muted)', fontWeight: 400 }}>
          {health ? `v${health.version}` : ''}
        </span>
      </h1>
      <p style={{ color: 'var(--muted)', marginTop: 8 }}>A dashcam for your coding agents.</p>

      {error && <p style={{ color: 'var(--accent)' }}>Can't reach the server: {error}</p>}

      {health && (
        <ul style={{ listStyle: 'none', padding: 0, marginTop: 32 }}>
          {health.sources.map((s) => (
            <li
              key={s.agent}
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                gap: 16,
                padding: '12px 16px',
                background: 'var(--surface)',
                border: '1px solid var(--border)',
                borderRadius: 8,
                marginBottom: 8,
              }}
            >
              <span>{s.agent}</span>
              <span className="mono" style={{ color: s.exists ? 'var(--ok)' : 'var(--muted)' }}>
                {s.exists ? s.path : 'not found'}
              </span>
            </li>
          ))}
        </ul>
      )}
    </main>
  )
}
