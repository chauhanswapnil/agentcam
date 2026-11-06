import { GitCommitHorizontal, GitPullRequest } from 'lucide-react'
import { Link, useLocation, useSearch } from 'wouter'
import { AGENTS } from '../lib/agents'
import { query, useApi, useIndexStatus, type CommitRow, type PrRow } from '../lib/api'
import { baseName, formatAgo, oneLine } from '../lib/format'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import s from './Views.module.css'
import { CsvButton } from '../ui/CsvButton'
import type { Column } from '../lib/csv'

const COMMIT_COLUMNS: Column<CommitRow>[] = [
  { label: 'when', value: (c) => c.ts },
  { label: 'message', value: (c) => c.message },
  { label: 'hash', value: (c) => c.hash },
  { label: 'branch', value: (c) => c.branch },
  { label: 'project', value: (c) => c.project_root },
  { label: 'agent', value: (c) => c.agent },
  { label: 'session', value: (c) => c.session_title },
  { label: 'session_id', value: (c) => c.session_id },
  { label: 'step', value: (c) => c.seq },
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

/** Every commit an agent session made, each linked to the step that made it. */
export function Commits() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const f = { range: p.get('range') ?? '30d', agent: p.get('agent') ?? '' }
  const set = (patch: Partial<typeof f>) => navigate(`/commits${query({ ...f, ...patch })}`, { replace: true })
  const gen = useIndexStatus().status?.generation ?? 0
  const prs = useApi<PrRow[]>(`/api/prs${query({ range: f.range, agent: f.agent })}`, gen)
  const { data } = useApi<CommitRow[]>(`/api/commits${query({ range: f.range, agent: f.agent })}`, gen)

  return (
    <div className={s.page}>
      <header className={s.head}>
        <div>
          <h1>Commits</h1>
          <p>
            Commits your agents made, found from their <code>git commit</code> commands. Click one to replay the step that
            made it. A hash shows when git printed one (<code>-q</code> hides it).
          </p>
        </div>
        <div className={s.controls}>
          <Segmented label="Agent" value={f.agent} onChange={(agent) => set({ agent })} options={AGENT_OPTIONS} />
          <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
        </div>
      </header>
      {!data ? (
        <Skeleton height={240} />
      ) : data.length === 0 ? (
        <Panel>
          <EmptyState icon={<GitCommitHorizontal />} title="No agent commits in this range" />
        </Panel>
      ) : (
        <Panel
          title={`${data.length} commits`}
          icon={<GitCommitHorizontal />}
          flush
          actions={<CsvButton name="commits" rows={data} columns={COMMIT_COLUMNS} />}
        >
          <div className={s.tableWrap}>
            <table className={s.table}>
              <thead>
                <tr>
                  <th>Message</th>
                  <th>Hash</th>
                  <th>Project</th>
                  <th>Agent</th>
                  <th>When</th>
                </tr>
              </thead>
              <tbody>
                {data.map((c) => (
                  <tr
                    key={`${c.session_id}:${c.seq}`}
                    className={s.rowBtn}
                    onClick={() => navigate(`/sessions/${c.session_id}?step=${c.seq}`)}
                    title={oneLine(c.session_title ?? '')}
                  >
                    <td style={{ whiteSpace: 'normal' }}>
                      <span className={s.name}>{c.message ?? '(no message)'}</span>
                      {c.branch && <span className={s.mono} style={{ color: 'var(--text-3)' }}>{c.branch}</span>}
                    </td>
                    <td className={s.mono}>{c.hash ?? '–'}</td>
                    <td>{c.project_root ? baseName(c.project_root) : '–'}</td>
                    <td>
                      <AgentLabel agent={c.agent} hideName />
                      <span className="sr-only">{AGENTS[c.agent]?.label}</span>
                    </td>
                    <td>{c.ts ? formatAgo(c.ts) : ''}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Panel>
      )}
      {prs.data && prs.data.length > 0 && (
        <Panel title={`${prs.data.length} pull request${prs.data.length === 1 ? '' : 's'}`} icon={<GitPullRequest />} flush>
          <div className={s.tableWrap}>
            <table className={s.table}>
              <thead>
                <tr>
                  <th>Pull request</th>
                  <th>Session</th>
                  <th>Agent</th>
                  <th>When</th>
                </tr>
              </thead>
              <tbody>
                {prs.data.map((p) => (
                  <tr key={p.url}>
                    <td>
                      <a href={p.url} target="_blank" rel="noreferrer" className={s.name}>
                        {p.repo ?? 'PR'}
                        {p.number != null ? ` #${p.number}` : ''}
                      </a>
                    </td>
                    <td>
                      <Link href={`/sessions/${p.session_id}?step=${p.seq}`}>{oneLine(p.title ?? 'Untitled session')}</Link>
                    </td>
                    <td>
                      <AgentLabel agent={p.agent} hideName />
                    </td>
                    <td>{p.ts ? formatAgo(p.ts) : ''}</td>
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
