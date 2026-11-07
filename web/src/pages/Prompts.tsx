import { MessageSquareText, Repeat } from 'lucide-react'
import { useState } from 'react'
import { Link, useLocation, useSearch } from 'wouter'
import { query, useApi, useDebounced, useIndexStatus } from '../lib/api'
import { baseName, formatAgo, formatNumber, oneLine } from '../lib/format'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import v from './Views.module.css'
import s from './Prompts.module.css'

/** Mirrors `prompts.rs` in agentcam-core. */
type PromptRow = {
  session_id: string
  seq: number
  ts: string | null
  text: string
  agent: 'claude-code' | 'codex'
  project_root: string | null
  session_title: string | null
}
type Report = {
  total: number
  prompts: PromptRow[]
  repeated: { text: string; count: number; sessions: number; projects: number; last_ts: string | null; session_id: string; seq: number }[]
}

const RANGES = [
  { value: '30d', label: '30 days' },
  { value: '90d', label: '90 days' },
  { value: 'all', label: 'All time' },
]

/**
 * Everything you typed to an agent, searchable, and the prompts you keep
 * typing. A repeat is worth a look: it might belong in a slash command, a
 * skill or AGENTS.md.
 */
export function Prompts() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const range = p.get('range') ?? '90d'
  const [text, setText] = useState(p.get('q') ?? '')
  const q = useDebounced(text, 250)
  const gen = useIndexStatus().status?.generation ?? 0
  const { data } = useApi<Report>(`/api/prompts${query({ range, q: q || undefined, limit: 300 })}`, gen)
  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Prompts</h1>
          <p>What you asked your agents, newest first. Prompts you typed in more than one session are grouped on the right.</p>
        </div>
        <div className={v.controls}>
          <input className={v.filter} placeholder="Search your prompts…" value={text} onChange={(e) => setText(e.target.value)} aria-label="Search your prompts" />
          <Segmented label="Time range" value={range} onChange={(r) => navigate(`/prompts${query({ range: r === '90d' ? undefined : r })}`, { replace: true })} options={RANGES} />
        </div>
      </header>
      {!data ? (
        <Skeleton height={400} />
      ) : (
        <div className={s.layout}>
          <Panel title={`${formatNumber(data.total)} prompts${q ? ` with “${q}”` : ''}`} icon={<MessageSquareText />} flush>
            {data.prompts.length === 0 ? (
              <EmptyState title="No prompts match" />
            ) : (
              <ul className={s.list}>
                {data.prompts.map((x) => (
                  <li key={`${x.session_id}:${x.seq}`}>
                    <Link href={`/sessions/${x.session_id}?step=${x.seq}`} className={s.item}>
                      <span className={s.text}>{x.text}</span>
                      <span className={s.meta}>
                        <AgentLabel agent={x.agent} hideName />
                        {x.project_root ? baseName(x.project_root) : 'No project'} · {x.ts ? formatAgo(x.ts) : ''}
                        {x.session_title ? ` · ${oneLine(x.session_title)}` : ''}
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </Panel>
          <Panel title="You keep asking" icon={<Repeat />} flush>
            {data.repeated.length === 0 ? (
              <EmptyState title="No repeats yet">Prompts typed in two or more sessions show here.</EmptyState>
            ) : (
              <ul className={s.list}>
                {data.repeated.map((r, i) => (
                  <li key={i}>
                    <Link href={`/sessions/${r.session_id}?step=${r.seq}`} className={s.item}>
                      <span className={s.repeatTop}>
                        <span className={s.count}>{r.count}×</span>
                        <span className={s.text}>{oneLine(r.text)}</span>
                      </span>
                      <span className={s.meta}>
                        {r.sessions} sessions · {r.projects} project{r.projects === 1 ? '' : 's'} · last {r.last_ts ? formatAgo(r.last_ts) : ''}
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            )}
            {data.repeated.length > 0 && (
              <p className={`${v.note} ${s.tip}`}>
                A prompt you type often could be a slash command or a skill, or a line in the project's AGENTS.md.
              </p>
            )}
          </Panel>
        </div>
      )}
    </div>
  )
}
