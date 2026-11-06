import { ListChecks, ShieldCheck } from 'lucide-react'
import type { Severity } from '../lib/api'
import { formatAgo, formatNumber } from '../lib/format'
import { EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { SeverityBadge } from '../ui/Severity'
import { Stat } from '../ui/Stat'
import v from '../pages/Views.module.css'
import { useTeamApi, type Governance as Data } from './api'
import { TeamHead } from './TeamHead'
import { useScope } from './scope'
import s from './Team.module.css'

const LEVELS: Severity[] = ['critical', 'high', 'medium', 'low']

export function Governance() {
  const { q } = useScope()
  const { data, error } = useTeamApi<Data>(`/api/team/governance${q}`)
  return (
    <div className={v.page}>
      <TeamHead title="Governance">
        What the risk rules found across the team's sessions. Secrets are masked on each laptop before anything is
        sent; only the rule, the step and a short excerpt arrive here.
      </TeamHead>
      {error && <p className={v.note}>{error}</p>}
      {!data ? (
        <Skeleton height={320} />
      ) : data.rules.length === 0 ? (
        <Panel>
          <EmptyState icon={<ShieldCheck />} title="No findings in this range" />
        </Panel>
      ) : (
        <>
          <div className={`${s.stats} ${s.stats4}`}>
            {LEVELS.map((l) => (
              <div className={s.stat} key={l}>
                <Stat label={l[0].toUpperCase() + l.slice(1)} value={formatNumber(data.by_severity[l] ?? 0)} sub={<SeverityBadge severity={l} />} />
              </div>
            ))}
          </div>
          <Panel title="Rules that fired" icon={<ListChecks />} flush>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Rule</th>
                    <th>Severity</th>
                    <th className={v.num}>Times</th>
                    <th className={v.num}>Sessions</th>
                    <th className={v.num}>Machines</th>
                    <th className={v.num}>Repos</th>
                  </tr>
                </thead>
                <tbody>
                  {data.rules.map((r) => (
                    <tr key={r.rule_id}>
                      <td className={v.mono}>{r.rule_id}</td>
                      <td>
                        <SeverityBadge severity={r.severity as Severity} />
                      </td>
                      <td className={v.num}>{formatNumber(r.count)}</td>
                      <td className={v.num}>{formatNumber(r.sessions)}</td>
                      <td className={v.num}>{formatNumber(r.machines)}</td>
                      <td className={v.num}>{formatNumber(r.projects)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>
          <Panel title="Latest findings, high and critical first" icon={<ShieldCheck />} flush>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Severity</th>
                    <th>Rule</th>
                    <th>What matched</th>
                    <th>Repo</th>
                    <th>Team</th>
                    <th>{data.show_people ? 'Person' : 'Machine'}</th>
                    <th>When</th>
                  </tr>
                </thead>
                <tbody>
                  {data.latest.map((f, i) => (
                    <tr key={i}>
                      <td>
                        <SeverityBadge severity={f.severity as Severity} />
                      </td>
                      <td className={v.mono}>{f.rule_id}</td>
                      <td className={`${v.mono} ${s.excerpt}`} title={f.excerpt}>
                        {f.excerpt}
                      </td>
                      <td className={`${v.mono} ${s.repo}`} title={f.project ?? ''}>
                        {f.project ?? '–'}
                      </td>
                      <td>{f.team}</td>
                      <td>{f.machine}</td>
                      <td>{f.ts ? formatAgo(f.ts) : ''}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>
          <p className={s.how}>
            The rules run on each laptop at index time, with that laptop's rules (built in plus its own rules.yaml).
            People's names show only when an admin turns them on in Settings; the change is logged.
          </p>
        </>
      )}
    </div>
  )
}
