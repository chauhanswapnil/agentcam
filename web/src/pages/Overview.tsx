import { Bot, CalendarDays, RotateCcw, FolderGit2, Inbox, RefreshCw, Rows3, TriangleAlert, X } from 'lucide-react'
import { useMemo, useState, type ReactNode } from 'react'
import { useLocation, useSearch } from 'wouter'
import { AGENTS, type AgentId } from '../lib/agents'
import {
  query,
  useApi,
  useIndexStatus,
  type Day,
  type Overview as OverviewData,
  type Project,
  type SessionPage,
  type SessionRow,
  type Stats,
} from '../lib/api'
import {
  baseName,
  dayLabel,
  formatAgo,
  formatCompact,
  formatDelta,
  formatHours,
  formatNumber,
  formatTime,
  formatCost,
  localDay,
  oneLine,
} from '../lib/format'
import { Badge } from '../ui/Badge'
import { SEVERITY_RANK } from '../lib/severity'
import { SeverityBadge } from '../ui/Severity'
import { Button } from '../ui/Button'
import { Heatmap } from '../ui/Heatmap'
import { ReportCardButton } from '../ui/ReportCard'
import { weekCard } from '../lib/reportCard'
import { ListRow } from '../ui/ListRow'
import { ResumeButton } from '../ui/SessionActions'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import { Sparkline } from '../ui/Sparkline'
import { Stat } from '../ui/Stat'
import { Tooltip } from '../ui/Tooltip'
import { TurnsPanel } from '../ui/TurnsPanel'
import type { TurnStats } from '../lib/api'
import s from './Overview.module.css'
import { CsvButton } from '../ui/CsvButton'
import type { Column } from '../lib/csv'

const SESSION_COLUMNS: Column<SessionRow>[] = [
  { label: 'session_id', value: (r) => r.id },
  { label: 'agent', value: (r) => r.agent },
  { label: 'title', value: (r) => r.title ?? r.first_prompt },
  { label: 'project', value: (r) => r.project_root },
  { label: 'folder', value: (r) => r.project_path },
  { label: 'branch', value: (r) => r.git_branch },
  { label: 'model', value: (r) => r.model },
  { label: 'started', value: (r) => r.started_at },
  { label: 'last_step', value: (r) => r.ended_at },
  { label: 'agent_time_ms', value: (r) => r.active_ms },
  { label: 'steps', value: (r) => r.event_count },
  { label: 'tool_calls', value: (r) => r.tool_call_count },
  { label: 'errored', value: (r) => r.error_count },
  { label: 'files', value: (r) => r.file_count },
  { label: 'subagents', value: (r) => r.subagents },
  { label: 'findings', value: (r) => r.finding_count },
  { label: 'commits', value: (r) => r.commit_count },
  { label: 'est_cost_usd', value: (r) => r.est_cost_usd },
]

const RANGES = [
  { value: '7d', label: '7 days' },
  { value: '30d', label: '30 days' },
  { value: '90d', label: '90 days' },
  { value: 'all', label: 'All time' },
]
const RANGE_WORDS: Record<string, string> = { '7d': 'previous 7 days', '30d': 'previous 30 days', '90d': 'previous 90 days' }
const AGENT_OPTIONS = [
  { value: '', label: 'All agents' },
  { value: 'claude-code', label: 'Claude Code' },
  { value: 'codex', label: 'Codex' },
]
const PAGE = 40

type Filters = { range: string; agent: string; project: string; day: string }

/** Filters live in the URL so reload, back and shared links keep them. */
function useFilters(): [Filters, (patch: Partial<Filters>) => void] {
  const search = useSearch()
  const [, navigate] = useLocation()
  const p = new URLSearchParams(search)
  const f: Filters = {
    range: p.get('range') ?? '30d',
    agent: p.get('agent') ?? '',
    project: p.get('project') ?? '',
    day: p.get('day') ?? '',
  }
  const set = (patch: Partial<Filters>) => {
    const next = { ...f, ...patch }
    navigate(`/${query({ ...next, range: next.range === '30d' ? undefined : next.range })}`, { replace: true })
  }
  return [f, set]
}

function delta(cur: number, prev: number | undefined, range: string) {
  if (prev === undefined || !RANGE_WORDS[range]) return undefined
  const d = formatDelta(cur, prev)
  if (!d) return prev === 0 && cur > 0 ? `none in the ${RANGE_WORDS[range]}` : undefined
  return d === 'same as' ? `same as ${RANGE_WORDS[range]}` : `${d} vs ${RANGE_WORDS[range]}`
}

function StatsStrip({ stats, prev, range }: { stats: Stats; prev?: Stats | null; range: string }) {
  const p = prev ?? undefined
  return (
    <div className={s.stats}>
      <div className={s.stat}>
        <Stat
          label="Agent time"
          value={formatHours(stats.active_ms)}
          sub={stats.wasted_ms > 0 ? formatHours(stats.wasted_ms) + ' on retries and reverts' : (delta(stats.active_ms, p?.active_ms, range) ?? 'idle gaps over 5 min left out')}
        />
      </div>
      <div className={s.stat}>
        <Stat label="Sessions" value={formatNumber(stats.sessions)} sub={delta(stats.sessions, p?.sessions, range) ?? `${stats.projects} projects`} />
      </div>
      <div className={s.stat}>
        <Stat
          label="Est. cost"
          value={formatCost(stats.cost_usd) + (stats.unpriced_calls ? '+' : '')}
          sub={stats.unpriced_calls ? formatNumber(stats.unpriced_calls) + ' calls with no price' : formatNumber(stats.prompts) + ' prompts · ' + formatNumber(stats.commits) + ' commits'}
        />
      </div>
      <div className={s.stat}>
        <Stat label="Tool calls" value={formatCompact(stats.tool_calls)} sub={`${formatNumber(stats.commands)} commands`} />
      </div>
      <div className={s.stat}>
        <Stat label="Files changed" value={formatNumber(stats.files_changed)} sub={`${formatNumber(stats.files_created)} new · ${formatNumber(stats.errors)} steps errored`} />
      </div>
    </div>
  )
}

function Projects({ projects, selected, onSelect }: { projects: Project[]; selected: string; onSelect: (p: string) => void }) {
  const [all, setAll] = useState(false)
  const shown = all ? projects : projects.slice(0, 6)
  if (!projects.length) return <EmptyState icon={<FolderGit2 />} title="No projects in this range" />
  return (
    <>
      {shown.map((p) => (
        <button
          key={p.path}
          type="button"
          className={`${s.project} ${p.path === selected ? s.projectOn : ''}`}
          onClick={() => onSelect(p.path === selected ? '' : p.path)}
          aria-pressed={p.path === selected}
          title={p.path || 'No folder recorded'}
        >
          <span className={s.projectName}>{p.path ? baseName(p.path) : 'Unknown folder'}</span>
          <span className={s.projectRight}>
            <Sparkline values={p.weekly_ms} />
            <span className={s.projectTime}>{formatHours(p.active_ms)}</span>
          </span>
          <span className={s.projectMeta}>
            <span className={s.marks}>
              {p.agents.map((a) => (
                <AgentLabel key={a} agent={a} hideName />
              ))}
            </span>
            {p.sessions} {p.sessions === 1 ? 'session' : 'sessions'}
            {p.cost_usd > 0 && <> · {formatCost(p.cost_usd)}</>}
            {p.last_active && <> · {formatAgo(p.last_active)}</>}
          </span>
        </button>
      ))}
      {projects.length > 6 && (
        <button type="button" className={s.more} onClick={() => setAll(!all)}>
          {all ? 'Show fewer' : `Show all ${projects.length}`}
        </button>
      )}
    </>
  )
}

function SessionList({ rows, onOpen }: { rows: SessionRow[]; onOpen: (id: string) => void }) {
  const groups = useMemo(() => {
    const out: { day: string; rows: SessionRow[]; ms: number }[] = []
    for (const r of rows) {
      const day = r.ended_at ? localDay(new Date(r.ended_at)) : 'unknown'
      const g = out.at(-1)
      if (g?.day === day) {
        g.rows.push(r)
        g.ms += r.active_ms
      } else out.push({ day, rows: [r], ms: r.active_ms })
    }
    return out
  }, [rows])

  return (
    <>
      {groups.map((g) => (
        <div key={g.day}>
          <div className={s.dayHead}>
            <span>{g.day === 'unknown' ? 'No date' : dayLabel(g.day)}</span>
            <span>{formatHours(g.ms)}</span>
          </div>
          {g.rows.map((r) => (
            <div key={r.id} className={s.sessionRow}>
              <ListRow
                onClick={() => onOpen(r.id)}
                leading={<AgentLabel agent={r.agent} hideName />}
                title={oneLine(r.title ?? r.first_prompt ?? 'Untitled session')}
                meta={
                  <>
                    <span>{r.project_root ? baseName(r.project_root) : 'Unknown folder'}</span>
                    {r.git_branch && (
                      <span className={s.branchWrap}>
                        <span>·</span>
                        <span className={s.branch}>{r.git_branch}</span>
                      </span>
                    )}
                    {r.ended_at && (
                      <>
                        <span>·</span>
                        <span>{formatTime(r.ended_at)}</span>
                      </>
                    )}
                  </>
                }
                trailing={
                  <span className={s.rowRight}>
                    {r.max_severity && SEVERITY_RANK[r.max_severity] >= 2 && (
                      <Tooltip content={`${r.finding_count} risk ${r.finding_count === 1 ? 'finding' : 'findings'}, worst: ${r.max_severity}`}>
                        <span className={s.hideSmall}>
                          <SeverityBadge severity={r.max_severity} />
                        </span>
                      </Tooltip>
                    )}
                    {r.wasted_ms > 0 && (
                      <Tooltip content={formatHours(r.wasted_ms) + ' on retries and reverts'}>
                        <Badge icon={<RotateCcw />} color="var(--status-serious)">
                          {formatHours(r.wasted_ms)}
                        </Badge>
                      </Tooltip>
                    )}
                    {r.error_count > 0 && (
                      <Tooltip content={`${r.error_count} ${r.error_count === 1 ? 'step' : 'steps'} errored`}>
                        <Badge icon={<TriangleAlert />} color="var(--status-critical)">
                          {r.error_count}
                        </Badge>
                      </Tooltip>
                    )}
                    {r.subagents > 0 && (
                      <Tooltip content={`${r.subagents} ${r.subagents === 1 ? 'subagent' : 'subagents'}`}>
                        <span className={`${s.steps} ${s.hideSmall}`}>
                          <Bot aria-hidden />
                          {r.subagents}
                        </span>
                      </Tooltip>
                    )}
                    <span className={`${s.steps} ${s.hideSmall}`}>{formatNumber(r.event_count)} steps</span>
                    {r.est_cost_usd != null && <span className={s.hideSmall}>{formatCost(r.est_cost_usd)}</span>}
                    <span className={s.dur}>{formatHours(r.active_ms)}</span>
                  </span>
                }
              />
              <span className={s.rowAction}>
                <ResumeButton agent={r.agent} id={r.id} cwd={r.project_path} compact />
              </span>
            </div>
          ))}
        </div>
      ))}
    </>
  )
}

export function Overview() {
  const [f, setF] = useFilters()
  const [, navigate] = useLocation()
  const { status, reindex } = useIndexStatus()
  const gen = status?.generation ?? 0
  const filterQuery = { range: f.range, agent: f.agent, project: f.project, day: f.day }
  // Page size resets whenever the filters change.
  const fkey = JSON.stringify(filterQuery)
  const [pages, setPages] = useState({ key: fkey, limit: PAGE })
  const limit = pages.key === fkey ? pages.limit : PAGE
  const showMore = () => setPages({ key: fkey, limit: limit + PAGE })
  const overview = useApi<OverviewData>(`/api/overview${query({ range: f.range, agent: f.agent, project: f.project, day: f.day })}`, gen)
  const activity = useApi<Day[]>(`/api/activity${query({ agent: f.agent, project: f.project })}`, gen)
  const sessions = useApi<SessionPage>(`/api/sessions${query({ ...filterQuery, range: f.day ? undefined : f.range, limit })}`, gen)

  // The weekly card always covers the last 7 days, whatever the range.
  const week = useApi<OverviewData>(`/api/overview${query({ range: '7d', agent: f.agent })}`, gen)
  const turns = useApi<TurnStats>(`/api/turns${query({ range: f.day ? undefined : f.range, agent: f.agent, project: f.project, day: f.day })}`, gen)
  const weekAudit = useApi<import('../lib/api').Audit>(`/api/audit${query({ range: '7d', agent: f.agent, severity: 'medium', limit: 1 })}`, gen)
  const indexing = status?.indexing
  const firstRun = indexing && !status?.last_report
  const o = overview.data

  return (
    <div className={s.page}>
      <header className={s.head}>
        <div>
          <h1>Overview</h1>
          <p>
            {status?.last_report
              ? `${formatNumber(status.last_report.files_seen)} logs indexed`
              : indexing
                ? 'Reading your agent logs…'
                : ' '}
          </p>
        </div>
        <div className={s.controls}>
          <Segmented label="Agent" value={f.agent} onChange={(agent) => setF({ agent })} options={AGENT_OPTIONS} />
          <Segmented label="Time range" value={f.range} onChange={(range) => setF({ range, day: '' })} options={RANGES} />
          <ReportCardButton
            label="This week"
            filename="agentcam-week.png"
            build={(redact) =>
              week.data && activity.data
                ? weekCard(week.data, activity.data, weekAudit.data, f.agent as AgentId | '', redact)
                : null
            }
          />
          <Tooltip content={indexing ? 'Indexing…' : 'Read new and changed logs'}>
            <Button icon variant="ghost" aria-label="Re-index" onClick={reindex} disabled={indexing}>
              <RefreshCw aria-hidden className={indexing ? s.spin : undefined} />
            </Button>
          </Tooltip>
        </div>
      </header>

      {o ? (
        <StatsStrip stats={o.stats} prev={f.day ? null : o.previous} range={f.range} />
      ) : (
        <div className={s.stats} style={{ height: 98 }} />
      )}

      <Panel
        title="Activity"
        icon={<CalendarDays />}
        actions={
          f.day ? (
            <Button size="sm" variant="ghost" onClick={() => setF({ day: '' })}>
              Clear day
            </Button>
          ) : undefined
        }
      >
        {activity.data ? (
          <Heatmap days={activity.data} selected={f.day || undefined} onSelect={(day) => setF({ day: day ?? '' })} />
        ) : (
          <Skeleton height={124} />
        )}
      </Panel>

      <div className={s.grid}>
        <Panel
          title={
            <>
              Sessions{sessions.data && <span style={{ color: 'var(--text-3)' }}> · {formatNumber(sessions.data.total)}</span>}
            </>
          }
          icon={<Rows3 />}
          flush
          actions={
            <>
              {sessions.data && <CsvButton name="sessions" rows={sessions.data.items} columns={SESSION_COLUMNS} />}
              {(f.project || f.day) && (
                <div className={s.filters}>
                  {f.day && (
                    <button type="button" className={s.chip} onClick={() => setF({ day: '' })} aria-label={`Remove day filter ${f.day}`}>
                      {dayLabel(f.day)} <X aria-hidden />
                    </button>
                  )}
                  {f.project && (
                    <button type="button" className={s.chip} onClick={() => setF({ project: '' })} aria-label="Remove project filter">
                      {baseName(f.project)} <X aria-hidden />
                    </button>
                  )}
                </div>
              )}
            </>
          }
        >
          {sessions.data?.items.length ? (
            <>
              <SessionList rows={sessions.data.items} onOpen={(id) => navigate(`/sessions/${id}`)} />
              {sessions.data.total > sessions.data.items.length && (
                <div className={s.loadMore}>
                  <Button size="sm" onClick={showMore} disabled={sessions.loading}>
                    Show more ({formatNumber(sessions.data.total - sessions.data.items.length)} left)
                  </Button>
                </div>
              )}
            </>
          ) : sessions.data || sessions.error ? (
            <EmptyState icon={<Inbox />} title={firstRun ? 'Reading your agent logs…' : 'No sessions match'}>
              {sessions.error ?? (firstRun ? 'This takes a few seconds the first time.' : 'Try a longer time range or clear the filters.')}
            </EmptyState>
          ) : (
            <div style={{ padding: 16, display: 'grid', gap: 14 }}>
              {[70, 55, 80, 45].map((w) => (
                <Skeleton key={w} width={`${w}%`} height={14} />
              ))}
            </div>
          )}
        </Panel>

        <div className={s.side}>
          <Panel title="Projects" icon={<FolderGit2 />} flush>
            {o ? (
              <Projects projects={o.projects} selected={f.project} onSelect={(project) => setF({ project })} />
            ) : (
              <div style={{ padding: 16, display: 'grid', gap: 14 }}>
                <Skeleton width="60%" height={14} />
                <Skeleton width="45%" height={14} />
              </div>
            )}
          </Panel>
          {o && o.stats.events > 0 && (
            <Panel title="Agents" flush>
              <AgentSplit filterAgent={f.agent as AgentId | ''} onSelect={(agent) => setF({ agent })} range={f.range} project={f.project} day={f.day} gen={gen} />
            </Panel>
          )}
          {turns.data && turns.data.turns > 0 && <TurnsPanel data={turns.data} />}
        </div>
      </div>
    </div>
  )
}

/** Agent time per agent, as a split bar with labels. */
function AgentSplit({
  filterAgent,
  onSelect,
  range,
  project,
  day,
  gen,
}: {
  filterAgent: AgentId | ''
  onSelect: (a: string) => void
  range: string
  project: string
  day: string
  gen: number
}) {
  const ids = Object.keys(AGENTS) as AgentId[]
  const claude = useApi<OverviewData>(`/api/overview${query({ range, project, day, agent: ids[0] })}`, gen)
  const codex = useApi<OverviewData>(`/api/overview${query({ range, project, day, agent: ids[1] })}`, gen)
  const rows = [
    { id: ids[0], ms: claude.data?.stats.active_ms ?? 0, sessions: claude.data?.stats.sessions ?? 0 },
    { id: ids[1], ms: codex.data?.stats.active_ms ?? 0, sessions: codex.data?.stats.sessions ?? 0 },
  ]
  const total = rows.reduce((n, r) => n + r.ms, 0) || 1
  return (
    <div className={s.agents}>
      <div className={s.split} aria-hidden>
        {rows.map((r) =>
          r.ms > 0 ? <span key={r.id} style={{ flex: r.ms, background: AGENTS[r.id].color }} /> : null,
        )}
      </div>
      {rows.map((r) => (
        <ListRowCompact
          key={r.id}
          active={filterAgent === r.id}
          onClick={() => onSelect(filterAgent === r.id ? '' : r.id)}
          left={<AgentLabel agent={r.id} />}
          right={`${formatHours(r.ms)} · ${Math.round((r.ms / total) * 100)}%`}
          sub={`${r.sessions} sessions`}
        />
      ))}
    </div>
  )
}

function ListRowCompact({ left, right, sub, active, onClick }: { left: ReactNode; right: string; sub: string; active: boolean; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} aria-pressed={active} className={`${s.agentRow} ${active ? s.agentRowOn : ''}`}>
      {left}
      <span className={s.agentRowRight}>
        <span>{right}</span>
        <span>{sub}</span>
      </span>
    </button>
  )
}
