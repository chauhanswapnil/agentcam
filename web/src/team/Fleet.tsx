import { Blocks, CircleCheck, Laptop, Settings2, Tag } from 'lucide-react'
import { Fragment } from 'react'
import { formatAgo, formatNumber } from '../lib/format'
import { FLAG_LABEL, ITEM_KIND, flagLabel } from '../lib/fleet'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import v from '../pages/Views.module.css'
import { useTeamApi, type Fleet as Data } from './api'
import { TeamHead } from './TeamHead'
import { useScope } from './scope'
import s from './Team.module.css'

type AgentId = 'claude-code' | 'codex'
const agentOf = (a: string): AgentId => (a === 'codex' ? 'codex' : 'claude-code')

export function Fleet() {
  const { q } = useScope()
  const { data, error } = useTeamApi<Data>(`/api/team/fleet${q}`)
  return (
    <div className={v.page}>
      <TeamHead title="Fleet" noRange>
        The agent setup on each machine that joined: versions, MCP servers, plugins, skills and hooks by name, and
        settings worth a look. Read from config files on each laptop; values, commands and URLs never leave it.
      </TeamHead>
      {error && <p className={v.note}>{error}</p>}
      {!data ? (
        <Skeleton height={320} />
      ) : data.machines.length === 0 ? (
        <Panel>
          <EmptyState icon={<Laptop />} title="No machines have joined yet" />
        </Panel>
      ) : (
        <>
          <Panel title="Settings to note" icon={<Settings2 />}>
            {data.flags.length === 0 ? (
              <p className={v.note}>Nothing to note on any machine.</p>
            ) : (
              <ul className={s.flags}>
                {[...data.flags].sort((a, b) => Number(!!FLAG_LABEL[a.id]?.good) - Number(!!FLAG_LABEL[b.id]?.good)).map((f) => (
                  <li key={`${f.agent}-${f.id}`}>
                    <span className={s.flagCount}>
                      {f.machines} of {data.machines.length}
                    </span>
                    <span>
                      <span className={s.flagName}>
                        <AgentLabel agent={agentOf(f.agent)} hideName /> {flagLabel(f.id)}
                        {FLAG_LABEL[f.id]?.good && (
                          <span className={s.good}>
                            <CircleCheck aria-hidden /> Set up
                          </span>
                        )}
                      </span>
                      <span className={s.flagNote}>{FLAG_LABEL[f.id]?.note}</span>
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </Panel>

          <Panel title="Agent versions in use" icon={<Tag />} flush>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Agent</th>
                    <th>Version</th>
                    <th className={v.num}>Machines</th>
                    <th className={v.num}>Sessions (30 days)</th>
                  </tr>
                </thead>
                <tbody>
                  {data.versions.map((x) => (
                    <tr key={`${x.agent}-${x.version}`}>
                      <td>
                        <AgentLabel agent={agentOf(x.agent)} />
                      </td>
                      <td className={v.mono}>{x.version ?? 'unknown'}</td>
                      <td className={v.num}>{formatNumber(x.machines)}</td>
                      <td className={v.num}>{formatNumber(x.sessions)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>

          <Panel title="Installed across machines" icon={<Blocks />} flush>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Agent</th>
                    <th>Detail</th>
                    <th className={v.num}>Machines</th>
                  </tr>
                </thead>
                <tbody>
                  {Object.keys(ITEM_KIND).map((k) => {
                    const rows = data.items.filter((i) => i.kind === k)
                    if (rows.length === 0) return null
                    return (
                      <Fragment key={k}>
                        <tr>
                          <td colSpan={4} className={s.groupRow}>
                            {ITEM_KIND[k]} · {rows.length}
                          </td>
                        </tr>
                        {rows.slice(0, 40).map((i) => (
                          <tr key={`${k}-${i.agent}-${i.name}-${i.detail}`}>
                            <td className={v.mono}>{i.name}</td>
                            <td>
                              <AgentLabel agent={agentOf(i.agent)} hideName />
                            </td>
                            <td>{i.detail ?? ''}</td>
                            <td className={v.num}>{formatNumber(i.machines)}</td>
                          </tr>
                        ))}
                      </Fragment>
                    )
                  })}
                </tbody>
              </table>
            </div>
          </Panel>

          <Panel title={`${data.machines.length} machines`} icon={<Laptop />} flush>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>{data.show_people ? 'Person' : 'Machine'}</th>
                    <th>Team</th>
                    <th>Agents</th>
                    <th>OS</th>
                    <th>agentcam</th>
                    <th className={v.num}>Sessions</th>
                    <th className={v.num}>Settings to look at</th>
                    <th>Last sync</th>
                  </tr>
                </thead>
                <tbody>
                  {data.machines.map((m, i) => (
                    <tr key={i}>
                      <td className={v.name}>{m.label}</td>
                      <td>{m.team}</td>
                      <td className={v.mono}>{m.agents.map(([a, ver]) => `${a === 'codex' ? 'Codex' : 'Claude Code'} ${ver ?? ''}`).join(', ')}</td>
                      <td>{m.os ?? ''}</td>
                      <td className={v.mono}>{m.agentcam_version ?? ''}</td>
                      <td className={v.num}>{formatNumber(m.sessions)}</td>
                      <td className={v.num} title={m.flags.map((f) => flagLabel(f.split(':')[1] ?? f)).join(', ')}>
                        {m.flags.filter((f) => !FLAG_LABEL[f.split(':')[1] ?? '']?.good).length}
                      </td>
                      <td>{m.last_sync ? formatAgo(m.last_sync) : 'never'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>
        </>
      )}
    </div>
  )
}
