import { Search as SearchIcon, SearchX } from 'lucide-react'
import { useLocation, useSearch } from 'wouter'
import { AGENTS } from '../lib/agents'
import { query, useApi, useDebounced, type Hit } from '../lib/api'
import { baseName, formatAgo, oneLine } from '../lib/format'
import { EmptyState, KindIcon, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import { Snippet } from '../ui/Snippet'
import s from './Search.module.css'

/** One "type" control that maps to the API's kind and category filters. */
const TYPES: { value: string; label: string; kind?: string; category?: string }[] = [
  { value: '', label: 'Everything' },
  { value: 'prompt', label: 'Prompts', kind: 'prompt' },
  { value: 'response', label: 'Replies', kind: 'response' },
  { value: 'command', label: 'Commands', category: 'command' },
  { value: 'edit', label: 'Edits', category: 'edit' },
  { value: 'read', label: 'Reads', category: 'read' },
  { value: 'web', label: 'Web', category: 'web' },
]
const RANGES = [
  { value: '30d', label: '30 days' },
  { value: '90d', label: '90 days' },
  { value: 'all', label: 'All time' },
]
const AGENT_OPTIONS = [
  { value: '', label: 'All agents' },
  { value: 'claude-code', label: 'Claude Code' },
  { value: 'codex', label: 'Codex' },
]

export function Search() {
  const params = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const f = {
    q: params.get('q') ?? '',
    type: params.get('type') ?? '',
    range: params.get('range') ?? 'all',
    agent: params.get('agent') ?? '',
  }
  const set = (patch: Partial<typeof f>) => {
    const n = { ...f, ...patch }
    navigate(`/search${query({ q: n.q, type: n.type, agent: n.agent, range: n.range === 'all' ? undefined : n.range })}`, {
      replace: true,
    })
  }
  const q = useDebounced(f.q.trim(), 200)
  const t = TYPES.find((x) => x.value === f.type) ?? TYPES[0]
  const res = useApi<Hit[]>(
    q.length >= 2
      ? `/api/search${query({ q, kind: t.kind, category: t.category, agent: f.agent, range: f.range, limit: 100 })}`
      : null,
  )

  return (
    <div className={s.page}>
      <label className={s.box}>
        <SearchIcon aria-hidden />
        <input
          className={s.input}
          value={f.q}
          onChange={(e) => set({ q: e.target.value })}
          placeholder="Search prompts, replies, commands, file paths…"
          autoFocus
          aria-label="Search"
        />
      </label>
      <div className={s.filters}>
        <Segmented label="Type" value={f.type} onChange={(type) => set({ type })} options={TYPES} />
        <Segmented label="Agent" value={f.agent} onChange={(agent) => set({ agent })} options={AGENT_OPTIONS} />
        <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
      </div>

      {q.length < 2 ? (
        <EmptyState icon={<SearchIcon />} title="Search every session">
          Words match anywhere in prompts, replies, commands, output and file paths. The last word matches as a prefix.
        </EmptyState>
      ) : !res.data ? (
        <Panel flush>
          <div style={{ padding: 16, display: 'grid', gap: 14 }}>
            {[80, 60, 70].map((w) => (
              <Skeleton key={w} width={`${w}%`} height={14} />
            ))}
          </div>
        </Panel>
      ) : res.data.length === 0 ? (
        <EmptyState icon={<SearchX />} title={`Nothing found for “${q}”`}>
          Try fewer words, or widen the filters.
        </EmptyState>
      ) : (
        <>
          <span className={s.count}>
            {res.data.length === 100 ? 'Top 100 matches' : `${res.data.length} ${res.data.length === 1 ? 'match' : 'matches'}`}
          </span>
          <Panel flush>
            {res.data.map((h) => (
              <button
                key={`${h.session_id}:${h.seq}`}
                type="button"
                className={s.hit}
                onClick={() => navigate(`/sessions/${h.session_id}?step=${h.seq}`)}
              >
                <KindIcon kind={h.kind} category={h.category} isError={h.is_error} />
                <span className={s.main}>
                  {/* The snippet may already be the title; don't print it twice. */}
                  {h.kind === 'tool_call' && h.snippet.split('\u0001').join('').split('\u0002').join('') !== h.title && (
                    <span className={s.title}>{h.title}</span>
                  )}
                  <Snippet text={h.snippet || h.title} className={s.snippet} />
                  <span className={s.meta}>
                    <span>{AGENTS[h.agent]?.label}</span>
                    <span>·</span>
                    <span>{h.project_root ? baseName(h.project_root) : 'Unknown folder'}</span>
                    {h.ts && (
                      <>
                        <span>·</span>
                        <span>{formatAgo(h.ts)}</span>
                      </>
                    )}
                    <span>·</span>
                    <span>{oneLine(h.session_title ?? 'Untitled session')}</span>
                  </span>
                </span>
              </button>
            ))}
          </Panel>
        </>
      )}
    </div>
  )
}
