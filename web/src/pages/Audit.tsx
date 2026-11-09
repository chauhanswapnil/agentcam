import { FileCheck2, ShieldAlert, ShieldCheck } from 'lucide-react'
import { useState } from 'react'
import { useLocation, useSearch } from 'wouter'
import { AGENTS } from '../lib/agents'
import { query, sendJson, useApi, useIndexStatus, type Audit as AuditData, type Approvals } from '../lib/api'
import { baseName, formatAgo, formatNumber, oneLine } from '../lib/format'
import { EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import { SEVERITY_COLOR } from '../lib/severity'
import { GuardPanel } from '../ui/GuardPanel'
import { ApprovalsPanel } from '../ui/ApprovalsPanel'
import { RuleEditor } from '../ui/RuleEditor'
import { FindingReview } from '../ui/FindingReview'
import { SeverityBadge } from '../ui/Severity'
import s from './Views.module.css'
import { CsvButton } from '../ui/CsvButton'
import type { Column } from '../lib/csv'
import type { FindingHit, RuleSummary } from '../lib/api'

/** Excerpts are stored with secrets already masked. */
const FINDING_COLUMNS: Column<FindingHit>[] = [
  { label: 'when', value: (h) => h.ts },
  { label: 'rule', value: (h) => h.rule_id },
  { label: 'title', value: (h) => h.title },
  { label: 'severity', value: (h) => h.severity },
  { label: 'excerpt', value: (h) => h.excerpt },
  { label: 'follows step', value: (h) => (h.related_seq != null ? String(h.related_seq + 1) : '') },
  { label: 'agent', value: (h) => h.agent },
  { label: 'project', value: (h) => h.project_root },
  { label: 'session', value: (h) => h.session_title },
  { label: 'session_id', value: (h) => h.session_id },
  { label: 'step', value: (h) => h.seq },
]
const RULE_COLUMNS: Column<RuleSummary>[] = [
  { label: 'rule', value: (r) => r.rule_id },
  { label: 'title', value: (r) => r.title },
  { label: 'severity', value: (r) => r.severity },
  { label: 'times', value: (r) => r.count },
  { label: 'sessions', value: (r) => r.sessions },
  { label: 'last', value: (r) => r.last_ts },
]

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
  const f = {
    range: p.get('range') ?? 'all',
    rule: p.get('rule') ?? '',
    severity: p.get('severity') ?? 'medium',
    status: p.get('status') ?? 'open',
  }
  // Bumped after marking a finding or muting a rule.
  const [rev, setRev] = useState(0)
  const set = (patch: Partial<typeof f>) => navigate(`/audit${query({ ...f, ...patch })}`, { replace: true })
  const { status } = useIndexStatus()
  const approvals = useApi<Approvals>(`/api/approvals${query({ range: f.range })}`, status?.generation ?? 0)
  const { data } = useApi<AuditData>(
    `/api/audit${query({ range: f.range, rule: f.rule, severity: f.severity, status: f.status, limit: 300 })}`,
    (status?.generation ?? 0) * 1000 + rev,
  )

  return (
    <div className={s.page}>
      <header className={s.head}>
        <div>
          <h1>Audit</h1>
          <p>
            Steps worth a second look: secrets in transcripts, sensitive files, dangerous commands, data sent out, and
            chains across steps, such as a secret read before an upload or a command taken from a tool result. Secrets
            are masked. Add your own rules below.
          </p>
        </div>
        <div className={s.controls}>
          <Segmented label="Severity" value={f.severity} onChange={(severity) => set({ severity })} options={MINS} />
          <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
        </div>
      </header>

      <GuardPanel />
      {approvals.data && <ApprovalsPanel data={approvals.data} />}

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
          <Panel
            title="Rules that fired"
            icon={<ShieldAlert />}
            flush
            actions={<CsvButton name="rules" rows={data.rules} columns={RULE_COLUMNS} />}
          >
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
                      <td className={s.wrapCell}>
                        <span className={s.name}>{r.title}</span>
                        <span className={s.mono} style={{ color: 'var(--text-3)' }}>
                          {r.rule_id}
                          {r.muted && ' · muted everywhere'}
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
            <Panel
              title={f.rule ? `Steps · ${f.rule}` : f.status === 'all' ? 'Latest findings' : 'To review'}
              flush
              actions={
                <span className={s.panelActions}>
                  {f.rule && <MuteRule ruleId={f.rule} muted={!!data.rules.find((r) => r.rule_id === f.rule)?.muted} onDone={() => setRev((n) => n + 1)} />}
                  <Segmented
                    label="Which findings"
                    value={f.status}
                    onChange={(status) => set({ status })}
                    options={[
                      { value: 'open', label: 'To review' },
                      { value: 'all', label: `All${data.reviewed ? ` · ${data.reviewed} looked at` : ''}` },
                    ]}
                  />
                  <CsvButton name="findings" rows={data.findings} columns={FINDING_COLUMNS} />
                </span>
              }
            >
              <div className={s.list}>
                {data.findings.length === 0 && (
                  <EmptyState title={f.status === 'open' && data.reviewed ? 'Nothing left to review' : 'No findings at this severity'}>
                    {f.status === 'open' && data.reviewed ? `${data.reviewed} findings were looked at or muted. Pick All to see them.` : undefined}
                  </EmptyState>
                )}
                {data.findings.map((h) => (
                  <div key={`${h.session_id}:${h.seq}:${h.rule_id}:${h.excerpt}`} className={`${s.refRow} ${h.review ? s.refLooked : ''}`}>
                    <button type="button" className={s.ref} onClick={() => navigate(`/sessions/${h.session_id}?step=${h.seq}`)}>
                      <span className={s.refTop}>
                        <ShieldAlert aria-hidden style={{ width: 13, height: 13, flex: 'none', color: SEVERITY_COLOR[h.severity] }} />
                        <span className={s.refTitle}>{h.excerpt}</span>
                      </span>
                      <span className={s.refMeta}>
                        {h.title}
                        {h.related_seq != null && h.related_seq !== h.seq && ` · follows step ${h.related_seq + 1}`} · {AGENTS[h.agent]?.label} · {h.project_root ? baseName(h.project_root) : '?'} ·{' '}
                        {h.ts ? formatAgo(h.ts) : ''} · {oneLine(h.session_title ?? '')}
                      </span>
                    </button>
                    <span className={s.refActions}>
                      <FindingReview
                        sessionId={h.session_id}
                        seq={h.seq}
                        ruleId={h.rule_id}
                        review={h.review}
                        project={h.project_root}
                        onDone={() => setRev((n) => n + 1)}
                      />
                    </span>
                  </div>
                ))}
              </div>
            </Panel>
          </div>
        </div>
      )}
      <ShareExports range={f.range} severity={f.severity} />
      <RuleEditor />
    </div>
  )
}

/** Evidence bundles and SIEM feeds for the range on screen. */
function ShareExports({ range, severity }: { range: string; severity: string }) {
  return (
    <Panel title="For your security team" icon={<FileCheck2 />}>
      <div className={s.exports}>
        <div>
          <span className={s.exportTitle}>Evidence bundle</span>
          <p className={s.exportText}>
            Every session in this range as one JSON file: who ran it and when, prompts, each step and how it was allowed, file
            changes, findings and commits. Secrets are masked, and a SHA-256 checksum shows whether the file was changed later
            (<code>agentcam evidence verify file.json</code>). One file holds up to 500 sessions; pick a shorter range for more.
          </p>
          <a className={s.exportLink} href={`/api/evidence${query({ range })}`} download>
            Download evidence
          </a>
        </div>
        <div>
          <span className={s.exportTitle}>Findings for a SIEM</span>
          <p className={s.exportText}>
            One finding per line at this severity and up. OCSF is the Detection Finding schema that Splunk, Sentinel and Security
            Lake read. To ship new ones on a schedule: <code>agentcam export findings --format ocsf --since &lt;last ts&gt;</code>.
          </p>
          <span className={s.exportLinks}>
            <a className={s.exportLink} href={`/api/findings/export${query({ range, severity, format: 'ndjson' })}`} download>
              NDJSON
            </a>
            <a className={s.exportLink} href={`/api/findings/export${query({ range, severity, format: 'ocsf' })}`} download>
              OCSF
            </a>
          </span>
        </div>
      </div>
    </Panel>
  )
}

/** Mute one rule in every project, or unmute it. */
function MuteRule({ ruleId, muted, onDone }: { ruleId: string; muted: boolean; onDone: () => void }) {
  const toggle = async () => {
    await sendJson('POST', '/api/reviews', { action: muted ? 'unmute' : 'mute', rule_id: ruleId, project: null, note: null })
    onDone()
  }
  return (
    <button type="button" className={s.exportLink} onClick={toggle} title={muted ? 'Show this rule again' : 'Hide this rule everywhere; its findings stay under All'}>
      {muted ? 'Unmute rule' : 'Mute everywhere'}
    </button>
  )
}
