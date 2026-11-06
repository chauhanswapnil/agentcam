import type { ReactNode } from 'react'
import { useLocation } from 'wouter'
import { query } from '../lib/api'
import { Segmented } from '../ui/Segmented'
import v from '../pages/Views.module.css'
import { useTeamApi, type Settings } from './api'
import { useScope } from './scope'

const RANGES = [
  { value: '7d', label: '7 days' },
  { value: '30d', label: '30 days' },
  { value: '90d', label: '90 days' },
  { value: 'all', label: 'All time' },
]

/** Page header with the range and team pickers. */
export function TeamHead({ title, children, noRange }: { title: string; children: ReactNode; noRange?: boolean }) {
  const [location, navigate] = useLocation()
  const { range, team } = useScope()
  const settings = useTeamApi<Settings>('/api/team/settings')
  const go = (patch: { range?: string; team?: string }) => {
    const next = { range, team, ...patch }
    navigate(`${location}${query({ range: next.range === '30d' ? undefined : next.range, team: next.team || undefined })}`, {
      replace: true,
    })
  }
  return (
    <header className={v.head}>
      <div>
        <h1>{title}</h1>
        <p>{children}</p>
      </div>
      <div className={v.controls}>
        {(settings.data?.teams.length ?? 0) > 1 && (
          <select className={v.select} aria-label="Team" value={team} onChange={(e) => go({ team: e.target.value })}>
            <option value="">All teams</option>
            {settings.data?.teams.map((t) => (
              <option key={t.id} value={String(t.id)}>
                {t.name}
              </option>
            ))}
          </select>
        )}
        {!noRange && <Segmented label="Time range" value={range} onChange={(r) => go({ range: r })} options={RANGES} />}
      </div>
    </header>
  )
}
