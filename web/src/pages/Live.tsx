import { ArrowUpRight, OctagonAlert, Radio, TriangleAlert, Workflow } from 'lucide-react'
import { STREAK } from '../lib/alerts'
import { memo, useEffect, useState } from 'react'
import { Link, useLocation, useSearch } from 'wouter'
import type { LimitHit, LiveSession, Limits } from '../lib/api'
import { query, useApi, useIndexStatus } from '../lib/api'
import { AGENTS } from '../lib/agents'
import { baseName, formatAgo, formatCompact, formatCost, formatHours, formatNumber, oneLine } from '../lib/format'
import { STATES, entrypointLabel, windowLabel } from '../lib/live'
import { AgentLabel, EmptyState, KindIcon, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import { UsageMeter } from '../ui/UsageMeter'
import { QuotaMeter } from '../ui/QuotaMeter'
import { AlertsPanel } from '../ui/AlertsPanel'
import v from './Views.module.css'
import s from './Live.module.css'

const RANGES = [
  { value: '15', label: '15 min' },
  { value: '60', label: '1 hour' },
  { value: '240', label: '4 hours' },
]

/** Re-render every `ms` so "idle for" and reset countdowns stay true. */
function useNow(ms: number) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), ms)
    return () => window.clearInterval(t)
  }, [ms])
  return now
}

const stepHref = (id: string, seq: number) => `/sessions/${id}${query({ step: seq })}`

/** What agents are doing now, and how full their usage windows are. */
export function Live() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const minutes = p.get('minutes') ?? '15'
  const gen = useIndexStatus().status?.generation ?? 0
  const now = useNow(5000)
  // Refetch when the index changes, and every 15s so states age out.
  const tick = Math.floor(now / 15_000)
  const live = useApi<LiveSession[]>(`/api/live${query({ minutes })}`, gen * 1e6 + tick)
  const limits = useApi<Limits>('/api/limits', gen)

  const working = live.data?.filter((x) => x.state !== 'quiet').length ?? 0
  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Live</h1>
          <p>Sessions with a step in the last {RANGES.find((r) => r.value === minutes)?.label ?? `${minutes} min`}. Updates as agents write their logs.</p>
        </div>
        <div className={v.controls}>
          <Segmented
            label="Time window"
            value={minutes}
            onChange={(m) => navigate(`/live${query({ minutes: m === '15' ? undefined : m })}`, { replace: true })}
            options={RANGES}
          />
        </div>
      </header>

      <Usage data={limits.data} now={now} />

      {!live.data ? (
        <Skeleton height={200} />
      ) : live.data.length === 0 ? (
        <Panel>
          <EmptyState icon={<Radio />} title="No agent has written a step recently">
            Start Claude Code or Codex and sessions show up here as they work.
          </EmptyState>
        </Panel>
      ) : (
        <section className={s.sessions} aria-label={`${live.data.length} sessions, ${working} active`}>
          {live.data.map((x) => (
            <LiveCard key={x.session.id} x={x} now={now} />
          ))}
        </section>
      )}

      <AlertsPanel projects={(live.data ?? []).map((x) => x.session.project_root ?? '').filter(Boolean)} />

      {limits.data && limits.data.hits.length > 0 && <Hits hits={limits.data.hits} total={limits.data.hits_30d} />}
    </div>
  )
}

function Usage({ data, now }: { data: Limits | null; now: number }) {
  if (!data) return <Skeleton height={120} />
  const claudeHits = data.hits.filter((h) => h.agent === 'claude-code')
  if (data.windows.length === 0 && data.hits.length === 0 && !data.claude) return null
  return (
    <section className={s.usage} aria-label="Usage windows">
      {data.windows.map((w) => (
        <UsageMeter
          key={`${w.agent}-${w.window}`}
          w={w}
          now={now}
          points={data.series.find((x) => x.agent === w.agent && x.window === w.window)?.points}
        />
      ))}
      {data.claude && <QuotaMeter q={data.claude} now={now} />}
      {claudeHits.length > 0 && !data.windows.some((w) => w.agent === 'claude-code') && (
        <div className={s.note}>
          <span className={s.noteTop}>
            <span className={s.agentDot} style={{ background: AGENTS['claude-code'].color }} aria-hidden />
            Claude Code
          </span>
          <span className={s.noteBig}>
            {claudeHits.filter((h) => now - new Date(h.ts).getTime() < 30 * 86_400_000).length}
            <span className={s.noteUnit}> limits reached in 30 days</span>
          </span>
          <span className={s.noteText}>
            Last: {windowLabel(claudeHits[0].window).toLowerCase()}, {formatAgo(claudeHits[0].ts)}
            {claudeHits[0].resets_text && `, reset ${claudeHits[0].resets_text}`}. Claude Code logs when a limit is
            reached, not how full each window is.
          </span>
        </div>
      )}
    </section>
  )
}

const LiveCard = memo(function LiveCard({ x, now }: { x: LiveSession; now: number }) {
  const st = STATES[x.state]
  const r = x.session
  const idle = Math.max(0, now - new Date(x.last_at).getTime())
  const from = entrypointLabel(r.entrypoint)
  const title = r.title ?? (r.first_prompt ? oneLine(r.first_prompt) : 'Untitled session')
  const latest = x.recent[0]
  return (
    <article className={`${s.card} ${x.state === 'quiet' ? s.quiet : ''}`}>
      <header className={s.cardTop}>
        <span className={s.state} title={st.hint}>
          <span className={`${s.pulse} ${x.state === 'working' ? s.pulsing : ''}`} style={{ background: st.color }} aria-hidden />
          {st.label}
          <span className={s.idle}>· {idle < 60_000 ? 'just now' : `${formatHours(idle)} ago`}</span>
        </span>
        <AgentLabel agent={r.agent} />
      </header>
      <Link href={latest ? stepHref(latest.session_id, latest.seq) : `/sessions/${r.id}`} className={s.title}>
        {title}
        <ArrowUpRight aria-hidden />
      </Link>
      <div className={s.meta}>
        {r.project_root && <span>{baseName(r.project_root)}</span>}
        {r.git_branch && <span className={s.mono}>{r.git_branch}</span>}
        {r.model && <span>{r.model}</span>}
        {from && <span>{from}</span>}
      </div>
      <dl className={s.stats}>
        <div>
          <dt>Agent time</dt>
          <dd>{formatHours(r.active_ms)}</dd>
        </div>
        <div>
          <dt>Steps</dt>
          <dd>{formatNumber(r.event_count)}</dd>
        </div>
        <div>
          <dt>Cost</dt>
          <dd>{(x.cost_usd ?? r.est_cost_usd) != null ? formatCost((x.cost_usd ?? r.est_cost_usd)!) : '–'}</dd>
        </div>
        <div>
          <dt>Context</dt>
          <dd>{x.context_tokens != null ? formatCompact(x.context_tokens) : '–'}</dd>
        </div>
      </dl>
      {(x.error_streak ?? 0) >= STREAK && (
        <div className={s.subs}>
          <TriangleAlert aria-hidden style={{ color: 'var(--status-warning)' }} />
          Last {x.error_streak} tool calls errored · worth a look
        </div>
      )}
      {x.active_subagents > 0 && (
        <div className={s.subs}>
          <Workflow aria-hidden />
          {x.active_subagents} subagent{x.active_subagents === 1 ? '' : 's'} working
        </div>
      )}
      <ol className={s.steps} aria-label="Latest steps">
        {x.recent.map((e) => (
          <li key={`${e.session_id}-${e.seq}`}>
            <Link href={stepHref(e.session_id, e.seq)} className={s.step}>
              <KindIcon kind={e.kind} category={e.category} isError={e.is_error} />
              <span className={s.stepTitle}>{e.title || e.tool_name || e.kind}</span>
              {e.session_id !== r.id && <span className={s.tag}>subagent</span>}
              <span className={s.stepTime}>{e.ts ? formatAgo(e.ts) : ''}</span>
            </Link>
          </li>
        ))}
      </ol>
    </article>
  )
})

function Hits({ hits, total }: { hits: LimitHit[]; total: number }) {
  return (
    <Panel title={`Limits reached · ${total} in 30 days`} icon={<OctagonAlert />} flush>
      <div className={v.tableWrap}>
        <table className={v.table}>
          <thead>
            <tr>
              <th>Limit</th>
              <th>Session</th>
              <th>Resets</th>
              <th>When</th>
            </tr>
          </thead>
          <tbody>
            {hits.map((h, i) => (
              <tr key={`${h.session_id}-${h.ts}-${i}`}>
                <td>
                  <span className={v.name}>
                    <AgentLabel agent={h.agent} hideName />
                    {windowLabel(h.window)}
                  </span>
                </td>
                <td>
                  <Link href={h.seq != null ? stepHref(h.session_id, h.seq) : `/sessions/${h.session_id}`} className={s.hitLink}>
                    {h.title ?? 'Untitled session'}
                  </Link>
                  {h.project_root && <span className={s.hitProject}>{baseName(h.project_root)}</span>}
                </td>
                <td>{h.resets_text ?? (h.resets_at ? new Date(h.resets_at).toLocaleString() : '–')}</td>
                <td>{formatAgo(h.ts)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Panel>
  )
}

