import { Ban, CircleCheck, Copy, Trash2, TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'wouter'
import { AGENTS } from '../lib/agents'
import { sendJson, useApi, type GuardInfo, type GuardRule, type HookSetup } from '../lib/api'
import { formatAgo } from '../lib/format'
import { Badge } from './Badge'
import { Button } from './Button'
import { EmptyState } from './Misc'
import { Panel } from './Panel'
import s from './GuardPanel.module.css'

/** Guard rules and the hook setup for Claude Code and Codex. */
export function GuardPanel() {
  const [rev, setRev] = useState(0)
  const { data } = useApi<GuardInfo>('/api/guard', rev)
  if (!data) return null
  const remove = async (r: GuardRule) => {
    await sendJson('DELETE', `/api/guard/${encodeURIComponent(r.id)}`)
    setRev((n) => n + 1)
  }
  const missing = data.hooks.filter((h) => !h.installed)

  return (
    <section id="guard" className={s.wrap}>
      <Panel
        title="Blocked next time"
        icon={<Ban />}
        flush
        actions={
          <span className={s.badges}>
            {data.hooks.map((h) => (
              <Badge
                key={h.agent}
                icon={h.installed ? <CircleCheck /> : <TriangleAlert />}
                color={h.installed ? 'var(--status-good)' : 'var(--status-warning)'}
              >
                {AGENTS[h.agent].label} {h.installed ? 'hook on' : 'hook off'}
              </Badge>
            ))}
          </span>
        }
      >
        {data.rules.length === 0 ? (
          <EmptyState icon={<Ban />} title="No guard rules yet">
            Open a risky step in a replay and choose Block next time.
          </EmptyState>
        ) : (
          data.rules.map((r) => (
            <div key={r.id} className={s.rule}>
              <span className={s.ruleText}>
                <span className={s.pattern}>
                  {r.tool} · {r.pattern}
                </span>
                <span className={s.meta}>
                  {r.reason}
                  {r.created ? ` · added ${formatAgo(r.created)}` : ''}
                  {r.from_session && (
                    <>
                      {' · '}
                      <Link href={`/sessions/${r.from_session}?step=${r.from_seq ?? 0}`}>from this step</Link>
                    </>
                  )}
                </span>
              </span>
              <Button icon size="sm" variant="ghost" aria-label={`Delete rule ${r.id}`} onClick={() => remove(r)}>
                <Trash2 aria-hidden />
              </Button>
            </div>
          ))
        )}
      </Panel>
      {missing.length > 0 && (
        <Panel title="Set up the hook">
          {missing.map((h) => (
            <Setup key={h.agent} h={h} />
          ))}
          <p className={s.small} style={{ marginTop: 16 }}>
            The hook runs <code>agentcam guard</code> before each tool call. It reads only the call and{' '}
            <code>{data.path}</code>, blocks matches with a message to the agent, and lets everything through if anything goes
            wrong. agentcam never edits your agent settings itself.
          </p>
        </Panel>
      )}
    </section>
  )
}

function Setup({ h }: { h: HookSetup }) {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    await navigator.clipboard.writeText(h.snippet).catch(() => {})
    setCopied(true)
  }
  return (
    <div className={s.setup}>
      <div className={s.setupHead}>
        <span>
          <b style={{ color: 'var(--text-1)' }}>{AGENTS[h.agent].label}</b>: add to <code>{h.settings_path}</code>
        </span>
        <Button size="sm" onClick={copy}>
          <Copy aria-hidden /> {copied ? 'Copied' : 'Copy'}
        </Button>
      </div>
      <pre className={s.pre}>{h.snippet}</pre>
      <p className={s.small}>{h.note}</p>
    </div>
  )
}
