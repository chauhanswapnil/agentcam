import { CalendarRange, FolderGit2, Sparkles, Users } from 'lucide-react'
import { formatCost, formatDelta, formatHours, formatNumber } from '../lib/format'
import { keptPct } from '../lib/lasted'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Stat } from '../ui/Stat'
import v from '../pages/Views.module.css'
import { followed, share, useTeamApi, type Insights as Data, type Sum } from './api'
import { SumTable } from './SumTable'
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
          <SumTable
            title="By week"
            icon={<CalendarRange />}
            csv="team-weeks"
            first="Week of"
            rows={[...data.weeks].reverse()}
            rowKey={(w) => w.week}
            name={(w) => w.week}
            cell={(w) => weekLabel(w.week)}
          />
          {data.by_team.length > 1 && (
            <SumTable
              title="By team"
              icon={<Users />}
              csv="team-teams"
              first="Team"
              rows={data.by_team}
              rowKey={(x) => x.name}
              name={(x) => x.name}
              cell={(x) => <span className={v.name}>{x.name}</span>}
            />
          )}
          <SumTable
            title="By repo"
            icon={<FolderGit2 />}
            csv="team-repos"
            first="Repo"
            rows={data.by_project}
            rowKey={(x) => x.name}
            name={(x) => x.name}
            cell={(x) => (
              <span className={`${v.mono} ${v.clip}`} title={x.name}>
                {x.name}
              </span>
            )}
          />
          <SumTable
            title="By agent and model"
            icon={<Sparkles />}
            csv="team-models"
            first="Model"
            rows={data.by_agent_model}
            rowKey={(m) => `${m.agent}-${m.model}`}
            name={(m) => `${m.agent} ${m.model ?? 'unknown'}`}
            cell={(m) => (
              <span className={v.name}>
                <AgentLabel agent={m.agent} hideName />
                {m.model ?? 'Unknown model'}
              </span>
            )}
          />
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
