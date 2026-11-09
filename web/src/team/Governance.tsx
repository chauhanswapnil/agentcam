import { ListChecks, ShieldCheck } from 'lucide-react'
import { useState } from 'react'
import type { Severity } from '../lib/api'
import { formatAgo, formatNumber } from '../lib/format'
import { severityNum, sortRows } from '../lib/sort'
import { CsvButton } from '../ui/CsvButton'
import { SortTh } from '../ui/SortTh'
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
  const [sort, setSort] = useState('')
  const th = (id: string, label: string, num = true) => <SortTh id={id} label={label} sort={sort} onSort={setSort} num={num} />
  const rules = data
    ? sortRows(data.rules, sort, {
        rule: (r) => r.rule_id,
        severity: (r) => severityNum(r.severity),
        count: (r) => r.count,
        sessions: (r) => r.sessions,
        machines: (r) => r.machines,
        projects: (r) => r.projects,
      })
    : []
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
          <Panel
            title="Rules that fired"
            icon={<ListChecks />}
            flush
            actions={
              <CsvButton
                name="team-rules"
                rows={rules}
                columns={[
                  { label: 'Rule', value: (r) => r.rule_id },
                  { label: 'Severity', value: (r) => r.severity },
                  { label: 'Times', value: (r) => r.count },
                  { label: 'Sessions', value: (r) => r.sessions },
                  { label: 'Machines', value: (r) => r.machines },
                  { label: 'Repos', value: (r) => r.projects },
                ]}
              />
            }
          >
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    {th('rule', 'Rule', false)}
                    {th('severity', 'Severity', false)}
                    {th('count', 'Times')}
                    {th('sessions', 'Sessions')}
                    {th('machines', 'Machines')}
                    {th('projects', 'Repos')}
                  </tr>
                </thead>
                <tbody>
                  {rules.map((r) => (
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
          <Panel
            title="Latest findings, high and critical first"
            icon={<ShieldCheck />}
            flush
            actions={
              <CsvButton
                name="team-findings"
                rows={data.latest}
                columns={[
                  { label: 'When', value: (f) => f.ts },
                  { label: 'Severity', value: (f) => f.severity },
                  { label: 'Rule', value: (f) => f.rule_id },
                  { label: 'What matched', value: (f) => f.excerpt },
                  { label: 'Repo', value: (f) => f.project },
                  { label: 'Team', value: (f) => f.team },
                  { label: data.show_people ? 'Person' : 'Machine', value: (f) => f.machine },
                  { label: 'Step', value: (f) => f.step },
                ]}
              />
            }
          >
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
