import { BadgeDollarSign, GitCompare, TrendingUp, Users } from 'lucide-react'
import { formatCost, formatDuration, formatNumber } from '../lib/format'
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

          <Panel title="Machines active each week" icon={<Users />} flush>
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

function Cohorts({ title, icon, rows, first }: { title: string; icon: React.ReactNode; rows: Cohort[]; first: string }) {
  if (rows.length === 0) return null
  return (
    <Panel title={title} icon={icon} flush>
      <div className={v.tableWrap}>
        <table className={v.table}>
          <thead>
            <tr>
              <th>{first}</th>
              <th className={v.num}>Sessions</th>
              <th className={v.num}>Machines</th>
              <th className={v.num}>Median time</th>
              <th className={v.num}>Median cost</th>
              <th className={v.num}>Errored / 100 steps</th>
              <th className={v.num}>In retries</th>
              <th className={v.num}>Lines still there</th>
              <th className={v.num}>Rules followed</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((c) => (
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
