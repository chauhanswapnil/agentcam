import { Ban, CircleCheck, Copy, Trash2, TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'wouter'
import { sendJson, useApi, type GuardInfo, type GuardRule } from '../lib/api'
import { formatAgo } from '../lib/format'
import { Badge } from './Badge'
import { Button } from './Button'
import { EmptyState } from './Misc'
import { Panel } from './Panel'

/** Guard rules and the Claude Code hook setup, for the Audit page. */
export function GuardPanel() {
  const [rev, setRev] = useState(0)
  const { data } = useApi<GuardInfo>('/api/guard', rev)
  const [copied, setCopied] = useState(false)
  if (!data) return null
  const remove = async (r: GuardRule) => {
    await sendJson('DELETE', `/api/guard/${encodeURIComponent(r.id)}`)
    setRev((n) => n + 1)
  }
  const copy = async () => {
    await navigator.clipboard.writeText(data.hook.snippet).catch(() => {})
    setCopied(true)
  }
  const pre = {
    margin: 0,
    padding: 12,
    overflowX: 'auto' as const,
    border: '1px solid var(--line-1)',
    borderRadius: 6,
    background: 'var(--bg-0)',
    color: 'var(--text-2)',
    fontFamily: 'var(--font-mono)',
    fontSize: 12,
  }

  return (
    <section id="guard" style={{ display: 'grid', gap: 16, scrollMarginTop: 72 }}>
      <Panel
        title="Blocked next time"
        icon={<Ban />}
        flush
        actions={
          data.hook.installed ? (
            <Badge icon={<CircleCheck />} color="var(--status-good)">
              Hook on
            </Badge>
          ) : (
            <Badge icon={<TriangleAlert />} color="var(--status-warning)">
              Hook not set up
            </Badge>
          )
        }
      >
        {data.rules.length === 0 ? (
          <EmptyState icon={<Ban />} title="No guard rules yet">
            Open a risky step in a replay and choose Block next time.
          </EmptyState>
        ) : (
          data.rules.map((r) => (
            <div key={r.id} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 16px', borderBottom: '1px solid var(--line-1)' }}>
              <span style={{ display: 'grid', gap: 2, flex: 1, minWidth: 0 }}>
                <span style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--text-1)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {r.tool} · {r.pattern}
                </span>
                <span style={{ fontSize: 12, color: 'var(--text-3)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {r.reason}
                  {r.created ? ` · added ${formatAgo(r.created)}` : ''}
                  {r.from_session && (
                    <>
                      {' · '}
                      <Link href={`/sessions/${r.from_session}?step=${r.from_seq ?? 0}`} style={{ textDecoration: 'underline' }}>
                        from this step
                      </Link>
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
      {!data.hook.installed && (
        <Panel title="Set up the Claude Code hook">
          <div style={{ display: 'grid', gap: 12, color: 'var(--text-2)' }}>
            <p>
              Add this to <code>{data.hook.settings_path ?? '~/.claude/settings.json'}</code> (merge it with any hooks you
              already have), then restart Claude Code. agentcam never edits your agent settings itself.
            </p>
            <pre style={pre}>{data.hook.snippet}</pre>
            <div>
              <Button size="sm" onClick={copy}>
                <Copy aria-hidden /> {copied ? 'Copied' : 'Copy'}
              </Button>
            </div>
            <p style={{ color: 'var(--text-3)', fontSize: 12 }}>
              The hook runs <code>agentcam guard</code> before each tool call. It reads only the call and{' '}
              <code>{data.path}</code>, blocks matches with a message to the agent, and lets everything through if anything
              goes wrong. Codex isn't supported yet.
            </p>
          </div>
        </Panel>
      )}
    </section>
  )
}
