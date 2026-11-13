import { Blocks, CircleAlert, FileText, Info, Plug, Settings2, Webhook, Wrench } from 'lucide-react'
import { useCallback, useRef, useState } from 'react'
import { Link } from 'wouter'
import { query, useApi, useIndexStatus } from '../lib/api'
import { useBehind } from '../lib/behind'
import { StepsDrawer } from '../ui/StepsDrawer'
import { formatNumber } from '../lib/format'
import { formatAgo } from '../lib/format'
import { Badge } from '../ui/Badge'
import { CopyButton } from '../ui/CopyButton'
import { OpenInEditor } from '../ui/SessionActions'
import { AgentLabel, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import v from './Views.module.css'
import s from './Setup.module.css'

/** Mirrors `setup.rs` in agentcam-core. */
type AgentId = 'claude-code' | 'codex'
type SetupData = {
  dirs: { home: string; claude: string; codex: string; claude_json: string; claude_from_env: boolean; codex_from_env: boolean }
  settings: {
    agent: AgentId
    scope: string
    project: string | null
    path: string
    error: string | null
    default_mode: string | null
    sandbox: string | null
    model: string | null
    allow: string[]
    deny: string[]
    ask: string[]
    env: string[]
  }[]
  memory: { scope: string; project: string | null; path: string; chars: number; tokens: number; imports: number }[]
  mcp: {
    agent: AgentId
    name: string
    transport: string
    target: string | null
    scope: string
    project: string | null
    defined_in: string
    calls_30d: number
    outside_content: boolean
    unpinned: boolean
    secret_keys: string[]
    plain_http: boolean
  }[]
  hooks: {
    agent: AgentId
    event: string
    matcher: string | null
    command: string
    program: string
    scope: string
    defined_in: string
    missing: boolean
    runs_30d: number
    errors_30d: number
  }[]
  skills: { name: string; source: string; chars: number; uses_90d: number }[]
  skill_listing_chars: number | null
  tools: {
    servers: {
      server: string
      tools: { tool: string; chars: number; flags: Flag[] }[]
      instructions_chars: number
      changes: { tool: string; first_seen: string | null; changed_at: string | null; before: string; after: string; new_flags: Flag[] }[]
      last_seen: string | null
      sessions: number
    }[]
    skills: { name: string; source: string; path: string; flags: [string, Flag][] }[]
  }
  checks: { id: string; level: 'look' | 'info'; title: string; detail: string; path: string | null }[]
}

type Flag = { id: string; level: 'look' | 'info'; title: string; excerpt: string | null }

/** Claude.ai connectors show up under an id; keep it short. */
const serverName = (n: string) => (/^[0-9a-f]{8}-[0-9a-f]{4}-/.test(n) ? `connector ${n.slice(0, 8)}` : n)

const home = (p: string) => p.replace(/^\/(Users|home)\/[^/]+/, '~')
const base = (p: string) => p.split('/').pop() ?? p

/**
 * How the agents are set up on this machine: settings, memory files, MCP
 * servers, hooks and skills, with checks worth a look. agentcam only reads
 * these files; change them in the agent's own settings.
 */
/** The id of the row a check is about, when it names one. */
function targetOf(data: SetupData, c: SetupData['checks'][number]): string | null {
  if (c.path) {
    const si = data.settings.findIndex((f) => f.path === c.path)
    if (si >= 0) return `setup-file-${si}`
    const mi = data.memory.findIndex((m) => m.path === c.path)
    if (mi >= 0) return `setup-memory-${mi}`
    const skill = data.skills.find((k) => c.path!.endsWith(`/skills/${k.name}`))
    if (skill) return `setup-skill-${skill.name}`
  }
  // "github has a secret written in its config": a server named first.
  if (c.id.startsWith('mcp_')) {
    const server = data.mcp.find((m) => c.title.startsWith(`${m.name} `))
    if (server) return `setup-mcp-${server.name}`
  }
  // "Stop hook points at a file that isn't there": the event named first.
  if (c.id.startsWith('hook_')) {
    const hi = data.hooks.findIndex((h) => c.title.startsWith(`${h.event} `) && (h.missing || !c.id.includes('missing')))
    if (hi >= 0) return `setup-hook-${hi}`
  }
  return null
}

export function Setup() {
  const gen = useIndexStatus().status?.generation ?? 0
  const { data } = useApi<SetupData>('/api/setup', gen)
  const [behind, openBehind, closeBehind] = useBehind()
  // The row a check pointed at, lit for a moment.
  const [lit, setLit] = useState<string | null>(null)
  const timer = useRef<number | undefined>(undefined)
  const show = useCallback((id: string) => {
    document.getElementById(id)?.scrollIntoView({ block: 'center' })
    setLit(id)
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => setLit(null), 1800)
  }, [])
  const row = (id: string) => (lit === id ? s.lit : undefined)
  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Setup</h1>
          <p>
            Your agents' settings, instruction files, MCP servers, hooks and skills on this machine, with a few things worth a
            look. agentcam reads these files and never changes them.
          </p>
        </div>
      </header>
      {!data ? (
        <Skeleton height={400} />
      ) : (
        <>
          <Panel title="Worth a look" icon={<CircleAlert />}>
            {data.checks.length === 0 ? (
              <p className={v.note}>Nothing to note.</p>
            ) : (
              <ul className={s.checks}>
                {[...data.checks]
                  .sort((a, b) => (a.level === b.level ? 0 : a.level === 'look' ? -1 : 1))
                  .map((c, i) => (
                    <li key={i} className={s.check}>
                      {c.level === 'look' ? (
                        <CircleAlert aria-hidden className={s.look} />
                      ) : (
                        <Info aria-hidden className={s.info} />
                      )}
                      <span>
                        <span className={s.checkTitle}>
                          {c.title}
                          <span className="sr-only">{c.level === 'look' ? ' (worth a look)' : ' (for your information)'}</span>
                        </span>
                        <span className={s.checkDetail}>{c.detail}</span>
                        {(() => {
                          const target = targetOf(data, c)
                          return (
                            target && (
                              <button type="button" className={s.showRow} onClick={() => show(target)}>
                                Show the row
                              </button>
                            )
                          )
                        })()}
                        {c.path && (
                          <span className={s.checkPathRow}>
                            <span className={s.checkPath}>{home(c.path)}</span>
                            <OpenInEditor path={c.path} />
                            <CopyButton text={c.path} label="Copy path" />
                          </span>
                        )}
                      </span>
                    </li>
                  ))}
              </ul>
            )}
          </Panel>

          <Panel title="Settings files" icon={<Settings2 />} flush>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>File</th>
                    <th>Scope</th>
                    <th>Mode</th>
                    <th className={v.num}>Allow</th>
                    <th className={v.num}>Ask</th>
                    <th className={v.num}>Deny</th>
                    <th>Environment (names)</th>
                  </tr>
                </thead>
                <tbody>
                  {data.settings.map((f, i) => (
                    <tr key={f.path} id={`setup-file-${i}`} className={row(`setup-file-${i}`)}>
                      <td>
                        <span className={v.name}>
                          <AgentLabel agent={f.agent} hideName />
                          <span className={v.mono} title={f.path}>
                            {f.project ? `${base(f.project)}/` : ''}
                            {home(f.path).split('/').slice(-2).join('/')}
                          </span>
                        </span>
                        {f.error && <span className={s.err}>{f.error}</span>}
                      </td>
                      <td>{f.scope}</td>
                      <td className={v.mono}>
                        {f.default_mode ?? 'default'}
                        {f.sandbox && ` · sandbox ${f.sandbox}`}
                      </td>
                      <td className={v.num} title={f.allow.join('\n')}>
                        {f.allow.length}
                      </td>
                      <td className={v.num} title={f.ask.join('\n')}>
                        {f.ask.length}
                      </td>
                      <td className={v.num} title={f.deny.join('\n')}>
                        {f.deny.length}
                      </td>
                      <td className={v.mono}>{f.env.join(', ')}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>

          <div className={s.grid}>
            <Panel title="Instruction and memory files" icon={<FileText />} flush>
              <div className={v.tableWrap}>
                <table className={v.table}>
                  <thead>
                    <tr>
                      <th>File</th>
                      <th className={v.num}>About</th>
                      <th className={v.num}>Imports</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.memory.map((m, i) => (
                      <tr key={m.path} id={`setup-memory-${i}`} className={row(`setup-memory-${i}`)}>
                        <td className={v.mono} title={m.path}>
                          {m.project ? `${base(m.project)}/` : '~/'}
                          {base(m.path)}
                        </td>
                        <td className={v.num}>{formatNumber(m.tokens)} tokens</td>
                        <td className={v.num}>{m.imports}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Panel>
            <Panel title="MCP servers" icon={<Plug />} flush>
              <div className={v.tableWrap}>
                <table className={v.table}>
                  <thead>
                    <tr>
                      <th>Server</th>
                      <th>Type</th>
                      <th>Where</th>
                      <th className={v.num}>Calls, 30 days</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.mcp.map((m, i) => (
                      <tr key={i} id={`setup-mcp-${m.name}`} className={row(`setup-mcp-${m.name}`)}>
                        <td>
                          <span className={v.name}>
                            <AgentLabel agent={m.agent} hideName />
                            {m.calls_30d > 0 ? (
                              <Link
                                href={`/tools${query({ tool: `mcp__${m.name}`, agent: m.agent, range: '30d' })}`}
                                className={s.cellLink}
                                title="Show this server's latest calls"
                              >
                                {m.name}
                              </Link>
                            ) : (
                              m.name
                            )}
                          </span>
                          {(m.outside_content || m.unpinned || m.secret_keys.length > 0 || m.plain_http) && (
                            <span className={s.tags}>
                              {m.outside_content && (
                                <span title="Its tools bring issues, messages or web pages into the session">
                                  <Badge>outside content</Badge>
                                </span>
                              )}
                              {m.unpinned && (
                                <span title="No version: every start runs the latest release">
                                  <Badge>latest release</Badge>
                                </span>
                              )}
                              {m.secret_keys.length > 0 && (
                                <span title={`Plain-text secret in: ${m.secret_keys.join(', ')}`}>
                                  <Badge dot color="var(--status-warning)">
                                    secret in config
                                  </Badge>
                                </span>
                              )}
                              {m.plain_http && (
                                <Badge dot color="var(--status-warning)">
                                  plain http
                                </Badge>
                              )}
                            </span>
                          )}
                        </td>
                        <td className={v.mono} title={m.target ?? ''}>
                          {m.transport}
                          {m.target ? ` · ${m.target}` : ''}
                        </td>
                        <td>{m.project ? base(m.project) : m.scope}</td>
                        <td className={v.num}>{formatNumber(m.calls_30d)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Panel>
          </div>

          <McpTools servers={data.tools.servers} />

          <Panel title="Hooks" icon={<Webhook />} flush>
            {data.hooks.length === 0 ? (
              <p className={`${v.note} ${s.pad}`}>No hooks set up.</p>
            ) : (
              <div className={v.tableWrap}>
                <table className={v.table}>
                  <thead>
                    <tr>
                      <th>Event</th>
                      <th>Matcher</th>
                      <th>Command</th>
                      <th>From</th>
                      <th className={v.num}>Runs, 30 days</th>
                      <th className={v.num}>Errored</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.hooks.map((h, i) => (
                      <tr key={i} id={`setup-hook-${i}`} className={row(`setup-hook-${i}`)}>
                        <td>
                          <span className={v.name}>
                            <AgentLabel agent={h.agent} hideName />
                            {h.event}
                          </span>
                        </td>
                        <td className={v.mono}>{h.matcher ?? 'all'}</td>
                        <td className={`${v.mono} ${s.cmd}`} title={h.command}>
                          {h.command}
                          {h.missing && <span className={s.err}>File not found</span>}
                        </td>
                        <td>{base(h.defined_in)}</td>
                        <td className={v.num}>
                          {h.runs_30d > 0 && h.program ? (
                            <button
                              type="button"
                              className={s.cellLink}
                              onClick={() => openBehind(`hook:${h.event}:${h.program}`)}
                              title="Show the steps where this hook ran"
                            >
                              {h.runs_30d}
                            </button>
                          ) : (
                            h.runs_30d
                          )}
                        </td>
                        <td className={v.num}>{h.errors_30d}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Panel>

          <Panel
            title={`Skills · ${data.skills.length}`}
            icon={<Blocks />}
            flush
            actions={
              data.skill_listing_chars ? (
                <span className={s.hint}>listing ≈ {formatNumber(Math.round(data.skill_listing_chars / 4))} tokens per session</span>
              ) : undefined
            }
          >
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Skill</th>
                    <th>From</th>
                    <th className={v.num}>Size</th>
                    <th className={v.num}>Runs, 90 days</th>
                  </tr>
                </thead>
                <tbody>
                  {[...data.skills]
                    .sort((a, b) => b.uses_90d - a.uses_90d || a.name.localeCompare(b.name))
                    .slice(0, 80)
                    .map((k, i) => (
                      <tr key={i} id={`setup-skill-${k.name}`} className={row(`setup-skill-${k.name}`)}>
                        <td className={v.mono}>
                          {k.name}
                          {data.tools.skills.some((f) => f.name === k.name) && (
                            <span className={s.tags} title={data.tools.skills.find((f) => f.name === k.name)?.flags.map(([file, f]) => `${file}: ${f.title}`).join('\n')}>
                              <Badge dot color="var(--status-warning)">
                                worth a look
                              </Badge>
                            </span>
                          )}
                        </td>
                        <td>{k.source.replace('claude-', 'Claude Code ').replace('codex', 'Codex')}</td>
                        <td className={v.num}>{formatNumber(Math.round(k.chars / 4))} tokens</td>
                        <td className={v.num}>{k.uses_90d}</td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
          </Panel>

          <p className={v.note}>
            Reading Claude Code from <code>{home(data.dirs.claude)}</code>
            {data.dirs.claude_from_env ? ' (CLAUDE_CONFIG_DIR)' : ''} and Codex from <code>{home(data.dirs.codex)}</code>
            {data.dirs.codex_from_env ? ' (CODEX_HOME)' : ''}. Projects come from sessions in the last 90 days.
          </p>
        </>
      )}
      {behind && <StepsDrawer what={behind} filter={{ range: '30d' }} onClose={closeBehind} />}
    </div>
  )
}

/**
 * MCP tools the agents were given in the last 90 days, from the logs: what
 * each server's tools and instructions say, flags on that text, and
 * descriptions that changed after they were first seen.
 */
function McpTools({ servers }: { servers: SetupData['tools']['servers'] }) {
  if (servers.length === 0) return null
  return (
    <Panel title={`MCP tools in your sessions · ${servers.length} servers`} icon={<Wrench />} flush>
      <p className={`${v.note} ${s.pad} ${s.intro}`}>
        What each server told the agent, read from the logs. Text aimed at the agent and descriptions that changed are listed under
        each server.
      </p>
      <ul className={s.servers}>
        {servers.map((x) => {
          const flagged = x.tools.filter((t) => t.flags.length > 0)
          const count = x.tools.filter((t) => t.tool !== '(instructions)').length
          return (
            <li key={x.server}>
              <details className={s.server}>
                <summary>
                  <span className={s.serverName} title={x.server}>
                    {serverName(x.server)}
                  </span>
                  <span className={s.serverMeta}>
                    {count} {count === 1 ? 'tool' : 'tools'} · {formatNumber(x.sessions)} {x.sessions === 1 ? 'session' : 'sessions'}
                    {x.last_seen && ` · ${formatAgo(x.last_seen)}`}
                  </span>
                  {flagged.length > 0 && (
                    <Badge dot color="var(--status-warning)">
                      {flagged.length} worth a look
                    </Badge>
                  )}
                  {x.changes.length > 0 && (
                    <Badge>
                      {x.changes.length} changed
                    </Badge>
                  )}
                </summary>
                <div className={s.serverBody}>
                  {flagged.map((t) => (
                    <div key={t.tool} className={s.toolFlag}>
                      <span className={s.toolName}>{t.tool}</span>
                      {t.flags.map((f) => (
                        <span key={f.id} className={s.flagLine}>
                          {f.title}
                          {f.excerpt && <code className={s.excerpt}>{f.excerpt}</code>}
                        </span>
                      ))}
                    </div>
                  ))}
                  {x.changes.map((c, i) => (
                    <div key={i} className={s.change}>
                      <span className={s.toolName}>
                        {c.tool === '(instructions)' ? 'Server instructions' : c.tool} changed
                        {c.changed_at && ` ${formatAgo(c.changed_at)}`}
                        {c.new_flags.length > 0 && ` · new text: ${c.new_flags.map((f) => f.title.toLowerCase()).join(', ')}`}
                      </span>
                      <code className={s.before}>{c.before || '(empty)'}</code>
                      <code className={s.after}>{c.after || '(empty)'}</code>
                    </div>
                  ))}
                  <span className={s.toolList}>
                    {x.tools
                      .filter((t) => t.tool !== '(instructions)')
                      .map((t) => t.tool)
                      .join(' · ')}
                    {x.instructions_chars > 0 && ` · instructions (${formatNumber(x.instructions_chars)} characters)`}
                  </span>
                </div>
              </details>
            </li>
          )
        })}
      </ul>
    </Panel>
  )
}
