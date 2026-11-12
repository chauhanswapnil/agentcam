import { Puzzle, TriangleAlert, Wrench } from 'lucide-react'
import { useLocation, useSearch } from 'wouter'
import { AGENTS } from '../lib/agents'
import { query, useApi, useIndexStatus, type Source, type StepRef, type ToolStat } from '../lib/api'
import { formatAgo, formatDuration, formatHours, formatNumber, oneLine } from '../lib/format'
import { EmptyState, KindIcon, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import s from './Views.module.css'
import { CsvButton } from '../ui/CsvButton'
import { sortRows } from '../lib/sort'
import { SortTh } from '../ui/SortTh'
import type { Column } from '../lib/csv'

const TOOL_COLUMNS: Column<ToolStat>[] = [
  { label: 'tool', value: (t) => t.tool_name },
  { label: 'category', value: (t) => t.category },
  { label: 'calls', value: (t) => t.calls },
  { label: 'errored', value: (t) => t.errors },
  { label: 'total_ms', value: (t) => t.total_ms },
  { label: 'sessions', value: (t) => t.sessions },
  { label: 'last', value: (t) => t.last_ts },
]

const RANGES = [
  { value: '7d', label: '7 days' },
  { value: '30d', label: '30 days' },
  { value: 'all', label: 'All time' },
]
const AGENT_OPTIONS = [
  { value: '', label: 'All agents' },
  { value: 'claude-code', label: 'Claude Code' },
  { value: 'codex', label: 'Codex' },
]

/** Every tool the agents used, and the latest calls of the one you pick. */
export function Tools() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const f = { range: p.get('range') ?? '30d', agent: p.get('agent') ?? '', tool: p.get('tool') ?? '', sort: p.get('sort') ?? 'calls' }
  const sort = f.sort
  const set = (patch: Partial<typeof f>) => navigate(`/tools${query({ ...f, ...patch })}`, { replace: true })
  const gen = useIndexStatus().status?.generation ?? 0
  const { data } = useApi<ToolStat[]>(`/api/tools${query({ range: f.range, agent: f.agent })}`, gen)
  const sources = useApi<Source[]>(`/api/sources${query({ range: f.range, agent: f.agent })}`, gen)
  const calls = useApi<StepRef[]>(f.tool ? `/api/tool-calls${query({ tool: f.tool, range: f.range, agent: f.agent, limit: 100 })}` : null, gen)
  const max = Math.max(1, ...(data ?? []).map((t) => t.calls))

  return (
    <div className={s.page}>
      <header className={s.head}>
        <div>
          <h1>Tool calls</h1>
          <p>Every tool your agents called, how often it errored and how long it took. Pick one to see its latest calls.</p>
        </div>
        <div className={s.controls}>
          <Segmented label="Agent" value={f.agent} onChange={(agent) => set({ agent })} options={AGENT_OPTIONS} />
          <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
        </div>
      </header>
      {!data ? (
        <Skeleton height={300} />
      ) : data.length === 0 ? (
        <Panel>
          <EmptyState icon={<Wrench />} title="No tool calls in this range" />
        </Panel>
      ) : (
        <div className={s.split}>
          <Panel
            title={`${formatNumber(data.length)} tools`}
            flush
            actions={<CsvButton name="tools" rows={data} columns={TOOL_COLUMNS} />}
          >
            <div className={s.tableWrap}>
              <table className={s.table}>
                <thead>
                  <tr>
                    <SortTh id="name" label="Tool" sort={sort} onSort={(sort) => set({ sort })} />
                    <SortTh id="calls" label="Calls" sort={sort} onSort={(sort) => set({ sort })} />
                    <SortTh id="errors" label="Errored" sort={sort} onSort={(sort) => set({ sort })} num />
                    <SortTh id="time" label="Time" sort={sort} onSort={(sort) => set({ sort })} num />
                    <SortTh id="avg" label="Avg" sort={sort} onSort={(sort) => set({ sort })} num />
                    <SortTh id="sessions" label="Sessions" sort={sort} onSort={(sort) => set({ sort })} num />
                  </tr>
                </thead>
                <tbody>
                  {sortRows(data, sort, {
                    name: (t) => t.tool_name,
                    calls: (t) => t.calls,
                    errors: (t) => t.errors / Math.max(1, t.calls),
                    time: (t) => t.total_ms,
                    avg: (t) => t.total_ms / Math.max(1, t.calls),
                    sessions: (t) => t.sessions,
                  }).map((t) => (
                    <tr
                      key={t.tool_name}
                      className={`${s.rowBtn} ${f.tool === t.tool_name ? s.rowOn : ''}`}
                      onClick={() => set({ tool: f.tool === t.tool_name ? '' : t.tool_name })}
                    >
                      <td>
                        <span className={s.name}>
                          <KindIcon kind="tool_call" category={t.category} />
                          <span className={`${s.mono} ${s.clip}`} style={{ color: 'var(--text-1)' }} title={t.tool_name}>
                            {t.tool_name}
                          </span>
                        </span>
                      </td>
                      <td>
                        <span className={s.bar} style={{ width: Math.max(2, (t.calls / max) * 56) }} />
                        {formatNumber(t.calls)}
                      </td>
                      <td className={s.num} style={{ color: t.errors ? 'var(--text-1)' : undefined }}>
                        {t.errors ? `${formatNumber(t.errors)} · ${Math.round((t.errors / t.calls) * 100)}%` : '–'}
                      </td>
                      <td className={s.num}>{t.total_ms ? formatHours(t.total_ms) : '–'}</td>
                      <td className={s.num}>{t.total_ms ? formatDuration(t.total_ms / t.calls) : '–'}</td>
                      <td className={s.num}>{formatNumber(t.sessions)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>
          <div className={s.sticky}>
            <Panel title={f.tool ? `Latest · ${f.tool}` : 'Latest calls'} flush>
              {!f.tool ? (
                <EmptyState title="Pick a tool">Its latest calls show here; click one to open the replay.</EmptyState>
              ) : (
                <div className={s.list}>
                  {(calls.data ?? []).map((c) => (
                    <button key={`${c.session_id}:${c.seq}`} type="button" className={s.ref} onClick={() => navigate(`/sessions/${c.session_id}?step=${c.seq}`)}>
                      <span className={s.refTop}>
                        {c.is_error && <TriangleAlert aria-hidden style={{ width: 13, height: 13, flex: 'none', color: 'var(--status-critical)' }} />}
                        <span className={s.refTitle}>{c.title}</span>
                      </span>
                      <span className={s.refMeta}>
                        {AGENTS[c.agent]?.label} · {c.ts ? formatAgo(c.ts) : ''}
                        {c.duration_ms != null ? ` · ${formatDuration(c.duration_ms)}` : ''} · {oneLine(c.session_title ?? '')}
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </Panel>
          </div>
        </div>
      )}
      {sources.data && sources.data.length > 0 && (
        <Panel title="Skills, plugins and MCP servers" icon={<Puzzle />} flush>
          <div className={s.tableWrap}>
            <table className={s.table}>
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Kind</th>
                  <th className={s.num}>Steps</th>
                  <th className={s.num}>Errored</th>
                  <th className={s.num}>Sessions</th>
                  <th>Last used</th>
                </tr>
              </thead>
              <tbody>
                {sources.data.map((x) => (
                  <tr key={`${x.kind}:${x.name}`} className={x.kind === 'mcp' && f.tool === `mcp__${x.name}` ? s.rowOn : undefined}>
                    <td className={s.mono}>
                      {x.kind === 'mcp' ? (
                        // An MCP server's calls are its tools: the same filter a tool row sets.
                        <button
                          type="button"
                          className={s.linkBtn}
                          title="Show this server's latest calls"
                          onClick={() => {
                            set({ tool: `mcp__${x.name}` })
                            window.scrollTo({ top: 0 })
                          }}
                        >
                          {x.name}
                        </button>
                      ) : (
                        x.name
                      )}
                    </td>
                    <td>{SOURCE_KIND[x.kind] ?? x.kind}</td>
                    <td className={s.num}>{formatNumber(x.steps)}</td>
                    <td className={s.num}>{x.errors ? formatNumber(x.errors) : '–'}</td>
                    <td className={s.num}>{formatNumber(x.sessions)}</td>
                    <td>{x.last_ts ? formatAgo(x.last_ts) : ''}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Panel>
      )}
    </div>
  )
}

const SOURCE_KIND: Record<string, string> = { skill: 'Skill', plugin: 'Plugin', app: 'App (MCP)', mcp: 'MCP server' }
