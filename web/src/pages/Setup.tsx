import { Blocks, CircleAlert, FileText, Info, Plug, Settings2, Webhook } from 'lucide-react'
import { useApi, useIndexStatus } from '../lib/api'
import { formatNumber } from '../lib/format'
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
  mcp: { agent: AgentId; name: string; transport: string; target: string | null; scope: string; project: string | null; defined_in: string; calls_30d: number }[]
  hooks: {
    agent: AgentId
    event: string
    matcher: string | null
    command: string
    scope: string
    defined_in: string
    missing: boolean
    runs_30d: number
    errors_30d: number
  }[]
  skills: { name: string; source: string; chars: number; uses_90d: number }[]
  skill_listing_chars: number | null
  checks: { id: string; level: 'look' | 'info'; title: string; detail: string; path: string | null }[]
}

const home = (p: string) => p.replace(/^\/(Users|home)\/[^/]+/, '~')
const base = (p: string) => p.split('/').pop() ?? p

/**
 * How the agents are set up on this machine: settings, memory files, MCP
 * servers, hooks and skills, with checks worth a look. agentcam only reads
 * these files; change them in the agent's own settings.
 */
export function Setup() {
  const gen = useIndexStatus().status?.generation ?? 0
  const { data } = useApi<SetupData>('/api/setup', gen)
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
                        {c.path && <span className={s.checkPath}>{home(c.path)}</span>}
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
                  {data.settings.map((f) => (
                    <tr key={f.path}>
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
                    {data.memory.map((m) => (
                      <tr key={m.path}>
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
                      <tr key={i}>
                        <td>
                          <span className={v.name}>
                            <AgentLabel agent={m.agent} hideName />
                            {m.name}
                          </span>
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
                      <tr key={i}>
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
                        <td className={v.num}>{h.runs_30d}</td>
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
                      <tr key={i}>
                        <td className={v.mono}>{k.name}</td>
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
    </div>
  )
}
