import { CalendarRange, FolderGit2, Sparkles, Users } from 'lucide-react'
import { formatCost, formatDelta, formatHours, formatNumber } from '../lib/format'
import { keptPct } from '../lib/lasted'
import { LastedBar } from '../ui/LastedBar'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Stat } from '../ui/Stat'
import v from '../pages/Views.module.css'
import { share, useTeamApi, type Insights as Data, type Sum } from './api'
import { TeamHead } from './TeamHead'
import { useScope, weekLabel } from './scope'
import s from './Team.module.css'

/** "+12% vs before", described, never judged. */
function vs(cur: number, prev: number | undefined) {
  if (prev === undefined) return undefined
  const d = formatDelta(cur, prev)
  if (!d) return prev === 0 && cur > 0 ? 'none before' : undefined
  return d === 'same as' ? 'same as before' : `${d} vs before`
}

const followed = (x: Sum) => (x.followed + x.not_followed > 0 ? x.followed / (x.followed + x.not_followed) : null)

/** Columns shared by the team, project and model tables. */
function SumCells({ x }: { x: Sum }) {
  const k = keptPct(x.lasted)
  return (
    <>
      <td className={v.num}>{formatNumber(x.sessions)}</td>
      <td className={v.num}>{formatNumber(x.machines)}</td>
      <td className={v.num}>{formatHours(x.active_ms)}</td>
      <td className={v.num}>{formatCost(x.cost_usd)}</td>
      <td className={v.num}>{share(x.active_ms ? x.retry_ms / x.active_ms : null)}</td>
      <td className={s.barCell}>
        <LastedBar c={x.lasted} />
      </td>
      <td className={v.num}>{k == null ? '–' : `${Math.round(k)}%`}</td>
      <td className={v.num}>{share(followed(x))}</td>
      <td className={v.num}>{formatNumber(x.findings)}</td>
    </>
  )
}

const HEAD = (
  <>
    <th className={v.num}>Sessions</th>
    <th className={v.num}>Machines</th>
    <th className={v.num}>Agent time</th>
    <th className={v.num}>Est. cost</th>
    <th className={v.num}>In retries</th>
    <th className={s.barCell}>Lines added</th>
    <th className={v.num}>Still there</th>
    <th className={v.num}>Rules followed</th>
    <th className={v.num}>Findings</th>
  </>
)

export function Insights() {
  const { q } = useScope()
  const { data, error } = useTeamApi<Data>(`/api/team/insights${q}`)
  return (
    <div className={v.page}>
      <TeamHead title="Insights">
        How the team works with agents: time, cost, what lasted and how sessions followed each repo's instructions.
        Subagents count toward their parent session.
      </TeamHead>
      {error && <p className={v.note}>{error}</p>}
      {!data ? (
        <Skeleton height={320} />
      ) : data.totals.sessions === 0 ? (
        <Panel>
          <EmptyState icon={<Users />} title="Nothing synced in this range yet">
            Laptops send metadata after <code>agentcam team join</code>. Make an invite on the Settings page.
          </EmptyState>
        </Panel>
      ) : (
        <>
          <Strip t={data.totals} p={data.previous ?? undefined} />
          <Panel title="By week" icon={<CalendarRange />} flush>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Week of</th>
                    {HEAD}
                  </tr>
                </thead>
                <tbody>
                  {[...data.weeks].reverse().map((w) => (
                    <tr key={w.week}>
                      <td>{weekLabel(w.week)}</td>
                      <SumCells x={w} />
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>
          {data.by_team.length > 1 && (
            <Panel title="By team" icon={<Users />} flush>
              <div className={v.tableWrap}>
                <table className={v.table}>
                  <thead>
                    <tr>
                      <th>Team</th>
                      {HEAD}
                    </tr>
                  </thead>
                  <tbody>
                    {data.by_team.map((t) => (
                      <tr key={t.name}>
                        <td className={v.name}>{t.name}</td>
                        <SumCells x={t} />
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Panel>
          )}
          <Panel title="By repo" icon={<FolderGit2 />} flush>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Repo</th>
                    {HEAD}
                  </tr>
                </thead>
                <tbody>
                  {data.by_project.map((p) => (
                    <tr key={p.name}>
                      <td className={`${v.mono} ${s.repo}`} title={p.name}>
                        {p.name}
                      </td>
                      <SumCells x={p} />
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>
          <Panel title="By agent and model" icon={<Sparkles />} flush>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Model</th>
                    {HEAD}
                  </tr>
                </thead>
                <tbody>
                  {data.by_agent_model.map((m) => (
                    <tr key={`${m.agent}-${m.model}`}>
                      <td>
                        <span className={v.name}>
                          <AgentLabel agent={m.agent} hideName />
                          {m.model ?? 'Unknown model'}
                        </span>
                      </td>
                      <SumCells x={m} />
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>
          <p className={s.how}>
            Cost is at API list prices, from the tokens in each log. Agent time leaves out idle gaps over 5 minutes.
            "Still there" is the share of lines agents added that are in the code today, as each laptop last checked.
            Rules come from each repo's AGENTS.md and CLAUDE.md.
          </p>
        </>
      )}
    </div>
  )
}

function Strip({ t, p }: { t: Sum; p?: Sum }) {
  const k = keptPct(t.lasted)
  const f = followed(t)
  return (
    <div className={s.stats}>
      <div className={s.stat}>
        <Stat label="Agent time" value={formatHours(t.active_ms)} sub={vs(t.active_ms, p?.active_ms) ?? 'idle gaps left out'} />
      </div>
      <div className={s.stat}>
        <Stat label="Sessions" value={formatNumber(t.sessions)} sub={vs(t.sessions, p?.sessions) ?? `${formatNumber(t.commits)} commits`} />
      </div>
      <div className={s.stat}>
        <Stat label="Machines" value={formatNumber(t.machines)} sub={vs(t.machines, p?.machines) ?? 'with a session'} />
      </div>
      <div className={s.stat}>
        <Stat
          label="Est. cost"
          value={formatCost(t.cost_usd) + (t.unpriced ? '+' : '')}
          sub={vs(t.cost_usd, p?.cost_usd) ?? 'at API list prices'}
        />
      </div>
      <div className={s.stat}>
        <Stat label="Lines still there" value={k == null ? '–' : `${Math.round(k)}%`} sub={`of ${formatNumber(t.lasted.added)} added`} />
      </div>
      <div className={s.stat}>
        <Stat
          label="Rules followed"
          value={share(f)}
          sub={t.followed + t.not_followed ? `${formatNumber(t.not_followed)} times worth a look` : 'no checkable rules'}
        />
      </div>
    </div>
  )
}
