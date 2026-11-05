import { ShieldAlert, ShieldCheck } from 'lucide-react'
import { useLocation, useSearch } from 'wouter'
import { AGENTS } from '../lib/agents'
import { query, useApi, useIndexStatus, type Audit as AuditData } from '../lib/api'
import { baseName, formatAgo, formatNumber, oneLine } from '../lib/format'
import { EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import { SEVERITY_COLOR } from '../lib/severity'
import { GuardPanel } from '../ui/GuardPanel'
import { SeverityBadge } from '../ui/Severity'
import s from './Views.module.css'

const RANGES = [
  { value: '30d', label: '30 days' },
  { value: '90d', label: '90 days' },
  { value: 'all', label: 'All time' },
]
const MINS = [
  { value: '', label: 'All' },
  { value: 'medium', label: 'Medium+' },
  { value: 'high', label: 'High+' },
]

/** What the risk rules found, by rule and as a feed of steps. */
export function Audit() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const f = { range: p.get('range') ?? 'all', rule: p.get('rule') ?? '', severity: p.get('severity') ?? 'medium' }
  const set = (patch: Partial<typeof f>) => navigate(`/audit${query({ ...f, ...patch })}`, { replace: true })
  const { status } = useIndexStatus()
  const { data } = useApi<AuditData>(
    `/api/audit${query({ range: f.range, rule: f.rule, severity: f.severity, limit: 300 })}`,
    status?.generation ?? 0,
  )

  return (
    <div className={s.page}>
      <header className={s.head}>
        <div>
          <h1>Audit</h1>
          <p>
            Risky steps found by the rules: secrets in transcripts, sensitive files, dangerous commands, installs and
            network use. Secrets are masked. Add your own rules in <code>rules.yaml</code> next to the index.
          </p>
        </div>
        <div className={s.controls}>
          <Segmented label="Severity" value={f.severity} onChange={(severity) => set({ severity })} options={MINS} />
          <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
        </div>
      </header>

      <GuardPanel />

      {!data ? (
        <Skeleton height={240} />
      ) : data.rules.length === 0 ? (
        <Panel>
          <EmptyState icon={<ShieldCheck />} title="Nothing found">
            No rule matched any step in this range.
          </EmptyState>
        </Panel>
      ) : (
        <div className={s.split}>
          <Panel title="Rules that fired" icon={<ShieldAlert />} flush>
            <div className={s.tableWrap}>
              <table className={s.table}>
                <thead>
                  <tr>
                    <th>Rule</th>
                    <th>Severity</th>
                    <th className={s.num}>Times</th>
                    <th className={s.num}>Sessions</th>
                    <th>Last</th>
                  </tr>
                </thead>
                <tbody>
                  {data.rules.map((r) => (
                    <tr
                      key={r.rule_id}
                      className={`${s.rowBtn} ${f.rule === r.rule_id ? s.rowOn : ''}`}
                      onClick={() => set({ rule: f.rule === r.rule_id ? '' : r.rule_id })}
                    >
                      <td>
                        <span className={s.name}>{r.title}</span>
                        <span className={s.mono} style={{ color: 'var(--text-3)' }}>
                          {r.rule_id}
                        </span>
                      </td>
                      <td>
                        <SeverityBadge severity={r.severity} />
                      </td>
                      <td className={s.num}>{formatNumber(r.count)}</td>
                      <td className={s.num}>{formatNumber(r.sessions)}</td>
                      <td>{r.last_ts ? formatAgo(r.last_ts) : ''}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>

          <div className={s.sticky}>
            <Panel title={f.rule ? `Steps · ${f.rule}` : 'Latest findings'} flush>
              <div className={s.list}>
                {data.findings.length === 0 && <EmptyState title="No findings at this severity" />}
                {data.findings.map((h) => (
                  <button
                    key={`${h.session_id}:${h.seq}:${h.rule_id}:${h.excerpt}`}
                    type="button"
                    className={s.ref}
                    onClick={() => navigate(`/sessions/${h.session_id}?step=${h.seq}`)}
                  >
                    <span className={s.refTop}>
                      <ShieldAlert aria-hidden style={{ width: 13, height: 13, flex: 'none', color: SEVERITY_COLOR[h.severity] }} />
                      <span className={s.refTitle}>{h.excerpt}</span>
                    </span>
                    <span className={s.refMeta}>
                      {h.title} · {AGENTS[h.agent]?.label} · {h.project_root ? baseName(h.project_root) : '?'} ·{' '}
                      {h.ts ? formatAgo(h.ts) : ''} · {oneLine(h.session_title ?? '')}
                    </span>
                  </button>
                ))}
              </div>
            </Panel>
          </div>
        </div>
      )}
    </div>
  )
}
