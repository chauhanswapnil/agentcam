import { Lightbulb } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useLocation, useSearch } from 'wouter'
import type { Cache, Costs, Fixes, Ledger, TimeBar as TimeBarData } from '../lib/api'
import { query, sendJson, useApi, useIndexStatus } from '../lib/api'
import { useBehind } from '../lib/behind'
import { CachePanel } from '../ui/CachePanel'
import { CostPanel } from '../ui/CostPanel'
import { FixCard } from '../ui/FixCard'
import { LedgerPanel } from '../ui/LedgerPanel'
import { EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import { StepsDrawer } from '../ui/StepsDrawer'
import { TimeBar } from '../ui/TimeBar'
import v from './Views.module.css'
import s from './Profile.module.css'

const RANGES = [
  { value: '7d', label: '7 days' },
  { value: '30d', label: '30 days' },
  { value: '90d', label: '90 days' },
  { value: 'all', label: 'All time' },
]
const AGENT_OPTIONS = [
  { value: '', label: 'All agents' },
  { value: 'claude-code', label: 'Claude Code' },
  { value: 'codex', label: 'Codex' },
]

/** Where the agent's time and tokens went, and the few changes that would give some back. */
export function Profile() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const f = { range: p.get('range') ?? '30d', agent: p.get('agent') ?? '' }
  const set = (patch: Partial<typeof f>) => navigate(`/profile${query({ ...f, ...patch })}`, { replace: true })
  const gen = useIndexStatus().status?.generation ?? 0
  const q = query(f)
  const time = useApi<TimeBarData>(`/api/profile${q}`, gen)
  // Bumped after a fix is marked, so the cards and the ledger reload.
  const [rev, setRev] = useState(0)
  const fixes = useApi<Fixes>(`/api/fixes${q}`, gen * 1000 + rev)
  const ledger = useApi<Ledger>('/api/ledger', gen * 1000 + rev)
  const mark = async (id: string, state: 'applied' | 'dismissed' | null) => {
    await sendJson('POST', '/api/fixes', { id, state })
    setRev((r) => r + 1)
  }
  const cache = useApi<Cache>(`/api/cache${q}`, gen)
  const cost = useApi<Costs>(`/api/cost${q}`, gen)
  const [behind, openBehind, closeBehind] = useBehind()
  // A link like /profile#cost lands on the panel once it is on the page.
  const hasCost = !!cost.data
  useEffect(() => {
    if (hasCost && location.hash) document.getElementById(location.hash.slice(1))?.scrollIntoView({ block: 'start' })
  }, [hasCost])

  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Profile</h1>
          <p>Where your agents' time and tokens went, and the few changes most likely to give some back.</p>
        </div>
        <div className={v.controls}>
          <Segmented label="Agent" value={f.agent} onChange={(agent) => set({ agent })} options={AGENT_OPTIONS} />
          <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
        </div>
      </header>

      {time.data ? time.data.total_ms > 0 && <TimeBar data={time.data} onPick={openBehind} /> : <Skeleton height={140} />}

      <Panel title="Fixes" icon={<Lightbulb />}>
        {!fixes.data ? (
          <Skeleton height={120} />
        ) : fixes.data.fixes.length > 0 ? (
          <>
            <div className={s.fixes}>
              {fixes.data.fixes.map((x) => (
                <FixCard
                  key={x.id}
                  fix={x}
                  onMark={(state) => mark(x.id, state)}
                  onSeeAll={() => openBehind(x.kind === 'cache' ? `cache:${x.id.split(':')[1]}` : `fix:${x.id}`)}
                  onWeek={(since, until) => openBehind(x.kind === 'cache' ? `cache:${x.id.split(':')[1]}` : `fix:${x.id}`, since, until)}
                />
              ))}
            </div>
            <p className={s.note}>
              Sizes are estimates from your logs. A fix shows only when it is worth about 10 minutes a week or 5% of the estimated cost. agentcam
              never edits your files: copy the text, or hand the prompt to your agent.
            </p>
          </>
        ) : (
          <EmptyState icon={<Lightbulb />} title={fixes.data.enough_data ? 'Nothing worth changing in this range' : 'Fixes need about a week of sessions'}>
            {fixes.data.enough_data
              ? 'No approval, command or cache pattern here costs enough to be worth your time.'
              : 'The time bar above works from the first session. Fixes wait for enough history to be sure of.'}
          </EmptyState>
        )}
      </Panel>

      {ledger.data && <LedgerPanel data={ledger.data} onClear={(id) => mark(id, null)} onOpen={openBehind} />}

      <div className={s.cols}>
        {cache.data && cache.data.calls > 0 && <CachePanel data={cache.data} onPick={openBehind} />}
        {cost.data && cost.data.calls > 0 && <CostPanel data={cost.data} onPick={openBehind} cacheLink={!!cache.data && cache.data.calls > 0} />}
      </div>
      {behind && <StepsDrawer what={behind} filter={{ ...f, since: p.get('since') ?? undefined, until: p.get('until') ?? undefined }} onClose={closeBehind} />}
    </div>
  )
}
