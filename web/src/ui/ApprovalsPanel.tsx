import { Check, Copy, ShieldCheck } from 'lucide-react'
import { createElement, useState } from 'react'
import { Link } from 'wouter'
import type { Approvals } from '../lib/api'
import { formatAgo, formatNumber } from '../lib/format'
import { APPROVALS, approvalInfo } from '../lib/turns'
import { Button } from './Button'
import { AgentLabel } from './Misc'
import { Panel } from './Panel'
import s from './ApprovalsPanel.module.css'

const ORDER = ['rule', 'auto', 'mode', 'config', 'you', 'rejected', 'blocked-auto', 'blocked-rule', 'blocked-safety']

/**
 * How Claude Code let tool calls run or stopped them, and allow rules that
 * would save asking. agentcam never edits settings: copy what you want.
 */
export function ApprovalsPanel({ data }: { data: Approvals }) {
  // Nothing is picked until you choose: these widen what runs without asking.
  const [picked, setPicked] = useState<Set<string>>(() => new Set())
  const [copied, setCopied] = useState(false)
  const total = Object.values(data.counts).reduce((a, b) => a + b, 0)
  if (total === 0) return null
  const json = JSON.stringify({ permissions: { allow: data.suggestions.map((x) => x.rule).filter((r) => picked.has(r)) } }, null, 2)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(json)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1600)
    } catch {
      /* clipboard blocked; the text is on screen to select */
    }
  }
  return (
    <Panel title="Approvals" icon={<ShieldCheck />}>
      <p className={s.lead}>How Claude Code let tool calls run or stopped them. Codex logs only its approval policy, shown on each turn.</p>
      <ul className={s.counts}>
        {ORDER.filter((k) => data.counts[k]).map((k) => {
          const a = APPROVALS[k]
          return (
            <li key={k}>
              {createElement(a.icon, { 'aria-hidden': true, style: { color: a.color } })}
              <span className={s.countN}>{formatNumber(data.counts[k])}</span>
              <span>{a.label.toLowerCase()}</span>
            </li>
          )
        })}
      </ul>

      {data.suggestions.length > 0 && (
        <section className={s.section}>
          <h4>Allow rules you could add</h4>
          <p className={s.lead}>
            Calls you approved by hand, or that auto mode had to check first. A rule lets them run without either. Broad
            commands (interpreters, shells, <code>rm</code>, <code>git push</code>) are left out. Pick what you trust,
            then add it to <code>permissions.allow</code> in your Claude Code settings.
          </p>
          <ul className={s.rules}>
            {data.suggestions.map((x) => (
              <li key={x.rule}>
                <label className={s.rule}>
                  <input
                    type="checkbox"
                    checked={picked.has(x.rule)}
                    onChange={(e) => {
                      const n = new Set(picked)
                      if (e.target.checked) n.add(x.rule)
                      else n.delete(x.rule)
                      setPicked(n)
                    }}
                  />
                  <code>{x.rule}</code>
                  <span className={s.ruleMeta}>
                    {[x.by_you ? `you approved ${x.by_you}` : null, x.by_auto ? `auto mode checked ${formatNumber(x.by_auto)}` : null]
                      .filter(Boolean)
                      .join(' · ')}
                  </span>
                </label>
              </li>
            ))}
          </ul>
          <pre className={s.json}>{json}</pre>
          <Button size="sm" onClick={copy} disabled={picked.size === 0}>
            {copied ? <Check aria-hidden /> : <Copy aria-hidden />} {copied ? 'Copied' : 'Copy JSON'}
          </Button>
        </section>
      )}

      {data.stopped.length > 0 && (
        <section className={s.section}>
          <h4>Calls that were stopped</h4>
          <ul className={s.stopped}>
            {data.stopped.slice(0, 12).map((x) => {
              const a = approvalInfo(x.approval)
              return (
                <li key={`${x.session_id}-${x.seq}`}>
                  <Link href={`/sessions/${x.session_id}?step=${x.seq}`} className={s.stop}>
                    <AgentLabel agent={x.agent} hideName />
                    {createElement(a.icon, { 'aria-hidden': true, style: { color: a.color, flex: 'none' } })}
                    <span className={s.stopWhy}>{a.label}</span>
                    <code className={s.stopTitle}>{x.title}</code>
                    <span className={s.stopWhen}>{x.ts ? formatAgo(x.ts) : ''}</span>
                  </Link>
                </li>
              )
            })}
          </ul>
        </section>
      )}
    </Panel>
  )
}
