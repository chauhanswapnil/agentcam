import { CircleCheck, CircleDashed, FolderSearch, Palette } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Link } from 'wouter'
import type { AgentId } from '../lib/agents'
import { Badge } from '../ui/Badge'
import { Button } from '../ui/Button'
import { ListRow } from '../ui/ListRow'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import s from './Home.module.css'

type LogSource = { agent: AgentId; path: string; exists: boolean }
type Health = { name: string; version: string; sources: LogSource[] }

/** Placeholder until the overview lands: shows which agent logs were found. */
export function Home() {
  const [health, setHealth] = useState<Health | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    fetch('/api/health')
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then(setHealth)
      .catch((e: Error) => setError(e.message))
  }, [])

  return (
    <div className={s.page}>
      <header className={s.head}>
        <h1>Overview</h1>
        <p>The activity heatmap, stats and session list arrive here next. For now: which agent logs agentcam can see.</p>
      </header>
      <Panel
        title="Log sources"
        icon={<FolderSearch />}
        flush
        actions={
          <Link href="/styleguide">
            <Button size="sm" variant="ghost">
              <Palette aria-hidden /> Styleguide
            </Button>
          </Link>
        }
      >
        {error && <EmptyState title="Can't reach the agentcam server">{error}</EmptyState>}
        {!health && !error && (
          <div style={{ padding: 16, display: 'grid', gap: 12 }}>
            <Skeleton width="50%" height={14} />
            <Skeleton width="35%" height={14} />
          </div>
        )}
        {health && (
          <div className={s.sources}>
            {health.sources.map((src) => (
              <ListRow
                key={src.agent}
                leading={<AgentLabel agent={src.agent} hideName />}
                title={<AgentLabel agent={src.agent} />}
                meta={<span className={s.path}>{src.path}</span>}
                trailing={
                  src.exists ? (
                    <Badge icon={<CircleCheck />} color="var(--status-good)">
                      Found
                    </Badge>
                  ) : (
                    <Badge icon={<CircleDashed />}>Not found</Badge>
                  )
                }
              />
            ))}
          </div>
        )}
      </Panel>
    </div>
  )
}
