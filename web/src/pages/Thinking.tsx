import { Brain, Search as SearchIcon } from 'lucide-react'
import { memo, useState } from 'react'
import { Link, useLocation, useSearch } from 'wouter'
import type { Hit, Thinking as ThinkingData, Thought } from '../lib/api'
import { query, useApi, useDebounced, useIndexStatus } from '../lib/api'
import { baseName, formatAgo, formatDuration, formatHours, formatNumber, oneLine } from '../lib/format'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import { Snippet } from '../ui/Snippet'
import v from './Views.module.css'
import s from './Thinking.module.css'

const RANGES = [
  { value: '7d', label: '7 days' },
  { value: '30d', label: '30 days' },
  { value: 'all', label: 'All time' },
]
const SORTS = [
  { value: 'longest', label: 'Longest' },
  { value: 'recent', label: 'Newest' },
]

/** The agents' thinking, on its own: longest or newest, or search it. */
export function Thinking() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const f = { range: p.get('range') ?? '30d', sort: p.get('sort') ?? 'longest', q: p.get('q') ?? '' }
  const set = (patch: Partial<typeof f>) => navigate(`/thinking${query({ ...f, ...patch })}`, { replace: true })
  const gen = useIndexStatus().status?.generation ?? 0
  const q = useDebounced(f.q.trim(), 200)
  const list = useApi<ThinkingData>(`/api/thinking${query({ range: f.range, sort: f.sort, limit: 60 })}`, gen)
  const hits = useApi<Hit[]>(q.length >= 2 ? `/api/search${query({ q, kind: 'thinking', range: f.range, limit: 60 })}` : null)
  const d = list.data

  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Thinking</h1>
          <p>What agents reasoned through before they acted. Claude Code records how long each block took on newer versions.</p>
        </div>
        <div className={v.controls}>
          <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
        </div>
      </header>

      {d && d.blocks > 0 && (
        <section className={s.stats} aria-label="Thinking totals">
          <div>
            <span>Blocks</span>
            <b>{formatNumber(d.blocks)}</b>
          </div>
          <div>
            <span>Time thinking</span>
            <b>{d.total_ms ? formatHours(d.total_ms) : '–'}</b>
          </div>
          {d.by_model.slice(0, 4).map((m) => (
            <div key={m.model ?? '-'}>
              <span>{m.model ?? 'Unknown model'}</span>
              <b>{formatNumber(m.blocks)}</b>
              <small>{m.timed ? `${formatDuration(m.total_ms / m.timed).replace(/ \d+s$/, '')} each` : 'time not recorded'}</small>
            </div>
          ))}
        </section>
      )}

      <div className={s.bar}>
        <label className={s.box}>
          <SearchIcon aria-hidden />
          <input value={f.q} onChange={(e) => set({ q: e.target.value })} placeholder="Search thinking…" aria-label="Search thinking" />
        </label>
        {q.length < 2 && <Segmented label="Order" value={f.sort} onChange={(sort) => set({ sort })} options={SORTS} />}
      </div>

      {q.length >= 2 ? (
        !hits.data ? (
          <Skeleton height={200} />
        ) : hits.data.length === 0 ? (
          <Panel>
            <EmptyState icon={<SearchIcon />} title={`No thinking matches “${q}”`} />
          </Panel>
        ) : (
          <Panel flush>
            {hits.data.map((h) => (
              <Link key={`${h.session_id}:${h.seq}`} href={`/sessions/${h.session_id}?step=${h.seq}`} className={s.hit}>
                <Snippet text={h.snippet || h.title} className={s.snippet} />
                <span className={s.meta}>
                  <AgentLabel agent={h.agent} hideName />
                  {h.project_root ? baseName(h.project_root) : ''} · {h.ts ? formatAgo(h.ts) : ''} · {oneLine(h.session_title ?? 'Untitled session')}
                </span>
              </Link>
            ))}
          </Panel>
        )
      ) : !d ? (
        <Skeleton height={300} />
      ) : d.items.length === 0 ? (
        <Panel>
          <EmptyState icon={<Brain />} title="No thinking in this range">
            Agents write thinking when their reasoning is shown in the log. Some models and versions keep it hidden.
          </EmptyState>
        </Panel>
      ) : (
        <div className={s.list}>
          {d.items.map((t) => (
            <ThoughtCard key={`${t.session_id}:${t.seq}`} t={t} />
          ))}
        </div>
      )}
    </div>
  )
}

const ThoughtCard = memo(function ThoughtCard({ t }: { t: Thought }) {
  const [open, setOpen] = useState(false)
  const long = t.text.length > 360
  return (
    <article className={s.card}>
      <header className={s.cardTop}>
        <AgentLabel agent={t.agent} hideName />
        <span className={s.cardTitle}>{oneLine(t.title ?? 'Untitled session')}</span>
        {t.duration_ms != null && <span className={s.dur}>{formatDuration(t.duration_ms)}</span>}
      </header>
      <p className={`${s.text} ${open ? '' : s.clamp}`}>{t.text}</p>
      <footer className={s.cardFoot}>
        <span>
          {[t.model, t.project_root ? baseName(t.project_root) : null, t.ts ? formatAgo(t.ts) : null].filter(Boolean).join(' · ')}
        </span>
        <span className={s.acts}>
          {long && (
            <button type="button" onClick={() => setOpen(!open)}>
              {open ? 'Show less' : 'Show more'}
            </button>
          )}
          <Link href={`/sessions/${t.session_id}?step=${t.seq}`}>Open in replay</Link>
        </span>
      </footer>
    </article>
  )
})
