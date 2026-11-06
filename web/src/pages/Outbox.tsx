import { Check, Inbox, Send, X } from 'lucide-react'
import { Fragment, useState } from 'react'
import { useLocation, useSearch } from 'wouter'
import { query, useApi, useIndexStatus } from '../lib/api'
import { formatAgo, formatCost, formatHours, formatNumber } from '../lib/format'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import s from './Views.module.css'

/** Mirrors `SessionMeta` in agentcam-core `share.rs`. */
type SessionMeta = {
  session_id: string
  parent_id: string | null
  agent: 'claude-code' | 'codex'
  project_remote: string | null
  project_name: string | null
  ended_at: string | null
  active_ms: number
  steps: Record<string, number>
  est_cost_usd: number | null
  findings: unknown[]
  commits: unknown[]
  files: unknown[]
}

type OutboxData = { schema_version: number; machine_id: string; sessions: SessionMeta[] }

const RANGES = [
  { value: '7d', label: '7 days' },
  { value: '30d', label: '30 days' },
  { value: 'all', label: 'All time' },
]

const SENT = [
  'Agent, model, versions, project (git remote and folder name), branch',
  'Start and end times, agent time, time in retries',
  'Step and tool-call counts, errored steps',
  'Tokens and estimated cost, peak context',
  'Findings: rule, severity, step and the masked excerpt',
  'Commits: hash and message',
  'File paths relative to the project, with read / edit / create / delete',
]
const NEVER = [
  'Prompts, replies and thinking',
  'Command text and output',
  'Tool inputs, file contents and diffs',
  'Full local paths and anything outside the project beyond a file name',
]

/**
 * What a team server would receive from this machine in metadata-only mode.
 * agentcam isn't connected to one, so nothing is sent; this is a preview of
 * the contract in `share.rs`.
 */
export function Outbox() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const range = p.get('range') ?? '7d'
  const gen = useIndexStatus().status?.generation ?? 0
  const { data } = useApi<OutboxData>(`/api/outbox${query({ range, limit: 200 })}`, gen)
  const [open, setOpen] = useState<string | null>(null)

  return (
    <div className={s.page}>
      <header className={s.head}>
        <div>
          <h1>Outbox</h1>
          <p>
            Exactly what a team server would receive from this machine, in metadata-only mode. agentcam isn't connected
            to a team server, so nothing is sent.
          </p>
        </div>
        <div className={s.controls}>
          <Segmented label="Time range" value={range} onChange={(r) => navigate(`/outbox${query({ range: r })}`, { replace: true })} options={RANGES} />
        </div>
      </header>

      <div className={s.split} style={{ gridTemplateColumns: '1fr 1fr' }}>
        <Panel title="Would be sent" icon={<Send />}>
          <ul className={s.list} style={{ margin: 0, paddingLeft: 0, listStyle: 'none', display: 'grid', gap: 8 }}>
            {SENT.map((t) => (
              <li key={t} style={{ display: 'flex', gap: 8, color: 'var(--text-2)' }}>
                <Check aria-hidden style={{ width: 14, flex: 'none', color: 'var(--status-good)' }} />
                {t}
              </li>
            ))}
          </ul>
        </Panel>
        <Panel title="Never sent" icon={<X />}>
          <ul style={{ margin: 0, paddingLeft: 0, listStyle: 'none', display: 'grid', gap: 8 }}>
            {NEVER.map((t) => (
              <li key={t} style={{ display: 'flex', gap: 8, color: 'var(--text-2)' }}>
                <X aria-hidden style={{ width: 14, flex: 'none', color: 'var(--text-3)' }} />
                {t}
              </li>
            ))}
          </ul>
          {data && (
            <p style={{ marginTop: 16, color: 'var(--text-3)', fontSize: 'var(--fs-xs)' }}>
              Machine id <code>{data.machine_id}</code> · schema v{data.schema_version}
            </p>
          )}
        </Panel>
      </div>

      {!data ? (
        <Skeleton height={240} />
      ) : data.sessions.length === 0 ? (
        <Panel>
          <EmptyState icon={<Inbox />} title="No sessions in this range" />
        </Panel>
      ) : (
        <Panel title={`${data.sessions.length} sessions`} icon={<Inbox />} flush>
          <div className={s.tableWrap}>
            <table className={s.table}>
              <thead>
                <tr>
                  <th>Project</th>
                  <th>Agent</th>
                  <th className={s.num}>Agent time</th>
                  <th className={s.num}>Steps</th>
                  <th className={s.num}>Cost</th>
                  <th className={s.num}>Findings</th>
                  <th className={s.num}>Files</th>
                  <th>When</th>
                </tr>
              </thead>
              <tbody>
                {data.sessions.map((m) => (
                  <Fragment key={m.session_id}>
                    <tr className={`${s.rowBtn} ${open === m.session_id ? s.rowOn : ''}`} onClick={() => setOpen(open === m.session_id ? null : m.session_id)}>
                      <td>
                        <span className={s.name}>
                          {m.project_name ?? '–'}
                          {m.parent_id && <span style={{ color: 'var(--text-3)', fontWeight: 400 }}>subagent</span>}
                        </span>
                        <span className={s.mono} style={{ color: 'var(--text-3)' }}>
                          {m.project_remote ?? 'no git remote'}
                        </span>
                      </td>
                      <td>
                        <AgentLabel agent={m.agent} hideName />
                      </td>
                      <td className={s.num}>{formatHours(m.active_ms)}</td>
                      <td className={s.num}>{formatNumber(Object.values(m.steps).reduce((a, b) => a + b, 0))}</td>
                      <td className={s.num}>{m.est_cost_usd != null ? formatCost(m.est_cost_usd) : '–'}</td>
                      <td className={s.num}>{m.findings.length}</td>
                      <td className={s.num}>{m.files.length}</td>
                      <td>{m.ended_at ? formatAgo(m.ended_at) : ''}</td>
                    </tr>
                    {open === m.session_id && (
                      <tr>
                        <td colSpan={8} style={{ whiteSpace: 'normal' }}>
                          <pre style={{ margin: 0, maxHeight: 420, overflow: 'auto', padding: 12, borderRadius: 6, background: 'var(--bg-0)', border: '1px solid var(--line-1)', color: 'var(--text-2)', fontFamily: 'var(--font-mono)', fontSize: 12 }}>
                            {JSON.stringify(m, null, 2)}
                          </pre>
                        </td>
                      </tr>
                    )}
                  </Fragment>
                ))}
              </tbody>
            </table>
          </div>
        </Panel>
      )}
    </div>
  )
}
