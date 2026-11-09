import { BadgeDollarSign, GitCompare, TrendingUp, Users } from 'lucide-react'
import { useState } from 'react'
import type { Column } from '../lib/csv'
import { formatCost, formatDuration, formatNumber } from '../lib/format'
import { sortRows } from '../lib/sort'
import { CsvButton } from '../ui/CsvButton'
import { SortTh } from '../ui/SortTh'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Sparkline } from '../ui/Sparkline'
import { Stat } from '../ui/Stat'
import v from '../pages/Views.module.css'
import { share, useTeamApi, type Cohort, type Value as Data } from './api'
import { TeamHead } from './TeamHead'
import { useScope, weekLabel } from './scope'
import s from './Team.module.css'

export function Value() {
  const { q } = useScope()
  const { data, error } = useTeamApi<Data>(`/api/team/value${q}`)
  return (
    <div className={v.page}>
      <TeamHead title="Value">
        What the agent work cost and what came of it: commits, pull requests and lines still in the code. Groups are
        described side by side, not ranked.
      </TeamHead>
      {error && <p className={v.note}>{error}</p>}
      {!data ? (
        <Skeleton height={320} />
      ) : data.totals.sessions === 0 ? (
        <Panel>
          <EmptyState icon={<BadgeDollarSign />} title="Nothing synced in this range yet" />
        </Panel>
      ) : (
        <>
          <div className={s.stats}>
            <div className={s.stat}>
              <Stat label="Est. cost" value={formatCost(data.totals.cost_usd)} sub="at API list prices" />
            </div>
            <div className={s.stat}>
              <Stat
                label="Per commit"
                value={data.cost_per_commit == null ? '–' : formatCost(data.cost_per_commit)}
                sub={`${formatNumber(data.totals.commits)} commits`}
              />
            </div>
            <div className={s.stat}>
              <Stat
                label="Per pull request"
                value={data.cost_per_pr == null ? '–' : formatCost(data.cost_per_pr)}
                sub={`${formatNumber(data.totals.prs)} PRs opened`}
              />
            </div>
            <div className={s.stat}>
              <Stat
                label="Lines kept per $"
                value={data.kept_lines_per_dollar == null ? '–' : formatNumber(Math.round(data.kept_lines_per_dollar))}
                sub={`${formatNumber(data.totals.lasted.kept)} lines still there`}
              />
            </div>
            <div className={s.stat}>
              <Stat
                label="Seats"
                value={formatNumber(data.seats.seats)}
                sub={data.seats.seat_cost_usd == null ? 'set a seat price in Settings' : `${formatCost(data.seats.seat_cost_usd)} in seats`}
              />
            </div>
            <div className={s.stat}>
              <Stat
                label="Use at API prices"
                value={formatCost(data.seats.api_value_usd)}
                sub={
                  data.seats.seat_cost_usd
                    ? `${(data.seats.api_value_usd / data.seats.seat_cost_usd).toFixed(1)}× the seat cost`
                    : 'for flat-rate plans, compare with seats'
                }
              />
            </div>
          </div>

          <Panel
            title="Machines active each week"
            icon={<Users />}
            flush
            actions={
              <CsvButton
                name="team-active-machines"
                rows={data.adoption}
                columns={[
                  { label: 'Team', value: (a) => a.team },
                  ...data.weeks.map((w, i) => ({ label: `Week of ${w}`, value: (a: (typeof data.adoption)[number]) => a.machines[i] ?? 0 })),
                ]}
              />
            }
          >
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Team</th>
                    <th className={s.sparkCell}>
                      {data.weeks.length ? `${weekLabel(data.weeks[0])} – ${weekLabel(data.weeks[data.weeks.length - 1])}` : ''}
                    </th>
                    <th className={v.num}>Latest week</th>
                    <th className={v.num}>Most in a week</th>
                  </tr>
                </thead>
                <tbody>
                  {data.adoption.map((a) => (
                    <tr key={a.team}>
                      <td className={v.name}>{a.team}</td>
                      <td className={s.sparkCell}>
                        <Sparkline values={a.machines.slice(-16)} width={160} />
                      </td>
                      <td className={v.num}>{a.machines[a.machines.length - 1] ?? 0}</td>
                      <td className={v.num}>{Math.max(0, ...a.machines)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>

          <Cohorts title="By model" icon={<TrendingUp />} rows={data.by_model} first="Model" />
          <Cohorts title="By agent version" icon={<GitCompare />} rows={data.by_version} first="Version" />

          <p className={s.how}>
            Per-commit and per-PR cost divide the whole range's cost by commits and pull requests that agent sessions
            made. Seats are machines with a session in range, a stand-in for people; seat cost spreads the monthly price
            you set over the range. Medians are per session. Groups differ in what they were used for, so read a gap as
            worth a look, not as a winner.
          </p>
        </>
      )}
    </div>
  )
}

const COHORT_SORT: Record<string, (c: Cohort) => number | string> = {
  label: (c) => c.label,
  sessions: (c) => c.sessions,
  machines: (c) => c.machines,
  time: (c) => c.median_active_ms ?? -1,
  cost: (c) => c.median_cost_usd ?? -1,
  errors: (c) => c.errors_per_100 ?? -1,
  retries: (c) => c.retry_share ?? -1,
  kept: (c) => c.kept_share ?? -1,
  followed: (c) => c.followed_share ?? -1,
}

const pctCell = (n: number | null | undefined) => (n == null ? null : Math.round(n * 1000) / 10)

function cohortColumns(first: string): Column<Cohort>[] {
  return [
    { label: first, value: (c) => c.label },
    { label: 'Agent', value: (c) => c.agent },
    { label: 'Sessions', value: (c) => c.sessions },
    { label: 'Machines', value: (c) => c.machines },
    { label: 'Median minutes', value: (c) => (c.median_active_ms == null ? null : Math.round(c.median_active_ms / 6000) / 10) },
    { label: 'Median cost (USD)', value: (c) => c.median_cost_usd },
    { label: 'Errored per 100 steps', value: (c) => c.errors_per_100 },
    { label: 'In retries (%)', value: (c) => pctCell(c.retry_share) },
    { label: 'Lines still there (%)', value: (c) => pctCell(c.kept_share) },
    { label: 'Rules followed (%)', value: (c) => pctCell(c.followed_share) },
  ]
}

function Cohorts({ title, icon, rows, first }: { title: string; icon: React.ReactNode; rows: Cohort[]; first: string }) {
  const [sort, setSort] = useState('')
  if (rows.length === 0) return null
  const shown = sortRows(rows, sort, COHORT_SORT)
  const th = (id: string, label: string, num = true) => <SortTh id={id} label={label} sort={sort} onSort={setSort} num={num} />
  return (
    <Panel
      title={title}
      icon={icon}
      flush
      actions={<CsvButton name={`team-${first.toLowerCase()}s`} rows={shown} columns={cohortColumns(first)} />}
    >
      <div className={v.tableWrap}>
        <table className={`${v.table} ${s.tight}`}>
          <thead>
            <tr>
              {th('label', first, false)}
              {th('sessions', 'Sessions')}
              {th('machines', 'Machines')}
              {th('time', 'Median time')}
              {th('cost', 'Median cost')}
              <SortTh id="errors" label="Errored /100" title="Steps that errored, per 100 steps" sort={sort} onSort={setSort} num />
              {th('retries', 'In retries')}
              {th('kept', 'Still there')}
              {th('followed', 'Rules followed')}
            </tr>
          </thead>
          <tbody>
            {shown.map((c) => (
              <tr key={`${c.agent}-${c.label}`}>
                <td>
                  <span className={v.name}>
                    <AgentLabel agent={c.agent} hideName />
                    {c.label}
                  </span>
                </td>
                <td className={v.num}>{formatNumber(c.sessions)}</td>
                <td className={v.num}>{formatNumber(c.machines)}</td>
                <td className={v.num}>{c.median_active_ms == null ? '–' : formatDuration(c.median_active_ms)}</td>
                <td className={v.num}>{c.median_cost_usd == null ? '–' : formatCost(c.median_cost_usd)}</td>
                <td className={v.num}>{c.errors_per_100 == null ? '–' : c.errors_per_100.toFixed(1)}</td>
                <td className={v.num}>{share(c.retry_share)}</td>
                <td className={v.num}>{share(c.kept_share)}</td>
                <td className={v.num}>{share(c.followed_share)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Panel>
  )
}
