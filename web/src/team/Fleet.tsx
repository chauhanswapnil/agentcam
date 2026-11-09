import { Blocks, CircleCheck, History, Laptop, ListChecks, Settings2, Tag } from 'lucide-react'
import { Fragment, useState } from 'react'
import { formatAgo, formatNumber } from '../lib/format'
import { sortRows } from '../lib/sort'
import { CsvButton } from '../ui/CsvButton'
import { SortTh } from '../ui/SortTh'
import { FLAG_LABEL, ITEM_KIND, TAG_LABEL, TAG_LOOK, flagLabel } from '../lib/fleet'
import { Badge } from '../ui/Badge'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import v from '../pages/Views.module.css'
import { useTeamApi, type BaselineView, type Fleet as Data } from './api'
import { TeamHead } from './TeamHead'
import { useScope } from './scope'
import s from './Team.module.css'

type AgentId = 'claude-code' | 'codex'
const agentOf = (a: string): AgentId => (a === 'codex' ? 'codex' : 'claude-code')

type Machine = Data['machines'][number]
/** Flags on a machine that aren't good news. */
const toLookAt = (m: Machine) => m.flags.filter((f) => !FLAG_LABEL[f.split(':')[1] ?? '']?.good).length
const agentsText = (m: Machine) => m.agents.map(([a, ver]) => `${a === 'codex' ? 'Codex' : 'Claude Code'} ${ver ?? ''}`.trim()).join(', ')

export function Fleet() {
  const { q } = useScope()
  const { data, error } = useTeamApi<Data>(`/api/team/fleet${q}`)
  const base = useTeamApi<BaselineView>(`/api/team/baseline${q}`)
  const [sort, setSort] = useState('')
  const th = (id: string, label: string, num = false) => <SortTh id={id} label={label} sort={sort} onSort={setSort} num={num} />
  const machines = data
    ? sortRows(data.machines, sort, {
        name: (m) => m.label,
        team: (m) => m.team,
        os: (m) => m.os ?? '',
        version: (m) => m.agentcam_version ?? '',
        sessions: (m) => m.sessions,
        flags: toLookAt,
        sync: (m) => m.last_sync ?? '',
      })
    : []
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
          {base.data && <BaselinePanel data={base.data} />}
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
                        {f.agent !== 'any' && <AgentLabel agent={agentOf(f.agent)} hideName />} {flagLabel(f.id)}
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
                            <td className={v.mono}>
                              {i.name}
                              {i.tags.length > 0 && (
                                <span className={s.tags}>
                                  {i.tags.map(([t, n]) => (
                                    <span key={t} title={`${n} of ${i.machines} machines`}>
                                      <Badge dot={TAG_LOOK.has(t)} color={TAG_LOOK.has(t) ? 'var(--status-warning)' : undefined}>
                                        {TAG_LABEL[t] ?? t}
                                      </Badge>
                                    </span>
                                  ))}
                                </span>
                              )}
                            </td>
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

          <Panel
            title={`${data.machines.length} machines`}
            icon={<Laptop />}
            flush
            actions={
              <CsvButton
                name="team-machines"
                rows={machines}
                columns={[
                  { label: data.show_people ? 'Person' : 'Machine', value: (m) => m.label },
                  { label: 'Team', value: (m) => m.team },
                  { label: 'Agents', value: agentsText },
                  { label: 'OS', value: (m) => m.os },
                  { label: 'agentcam', value: (m) => m.agentcam_version },
                  { label: 'Sessions', value: (m) => m.sessions },
                  { label: 'Settings to look at', value: (m) => m.flags.map((f) => flagLabel(f.split(':')[1] ?? f)).join('; ') },
                  { label: 'Joined', value: (m) => m.joined_at },
                  { label: 'Last sync', value: (m) => m.last_sync },
                ]}
              />
            }
          >
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    {th('name', data.show_people ? 'Person' : 'Machine')}
                    {th('team', 'Team')}
                    <th>Agents</th>
                    {th('os', 'OS')}
                    {th('version', 'agentcam')}
                    {th('sessions', 'Sessions', true)}
                    {th('flags', 'Settings to look at', true)}
                    {th('sync', 'Last sync')}
                  </tr>
                </thead>
                <tbody>
                  {machines.map((m, i) => (
                    <tr key={`${m.label}-${i}`}>
                      <td className={v.name}>{m.label}</td>
                      <td>{m.team}</td>
                      <td className={v.mono}>{agentsText(m)}</td>
                      <td>{m.os ?? ''}</td>
                      <td className={v.mono}>{m.agentcam_version ?? ''}</td>
                      <td className={v.num}>{formatNumber(m.sessions)}</td>
                      <td className={v.num} title={m.flags.map((f) => flagLabel(f.split(':')[1] ?? f)).join(', ')}>
                        {toLookAt(m)}
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

/**
 * The team's setup baseline and the machines that differ from it, each
 * difference with since when. A list to work through, not a ranking.
 */
function BaselinePanel({ data }: { data: BaselineView }) {
  if (!data.set) {
    return (
      <Panel title="Baseline" icon={<ListChecks />}>
        <p className={v.note}>
          No baseline yet. An admin can set one in Settings: the setup every machine should have (say, the guard hook) and
          what none should (approvals skipped by default, a secret in an MCP config). This page then lists the machines that
          differ and since when.
        </p>
      </Panel>
    )
  }
  return (
    <>
      <Panel
        title="Baseline"
        icon={<ListChecks />}
        actions={
          <span className={s.baselineCount}>
            {data.matching} of {data.machines} machines match
          </span>
        }
        flush
      >
        {data.drift.length === 0 ? (
          <p className={`${v.note} ${s.pad}`}>Every machine matches the baseline.</p>
        ) : (
          <>
            <ul className={s.ruleCounts}>
              {data.by_rule.map(([rule, n]) => (
                <li key={rule}>
                  <span className={s.flagCount}>{n}</span> {rule}
                </li>
              ))}
            </ul>
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Machine</th>
                    <th>Team</th>
                    <th>Differs from the baseline</th>
                    <th>Last sync</th>
                  </tr>
                </thead>
                <tbody>
                  {data.drift.map((m) => (
                    <tr key={m.machine} className={s.topRow}>
                      <td>
                        <span className={v.name}>{m.machine}</span>
                      </td>
                      <td>{m.team}</td>
                      <td>
                        <ul className={s.diffs}>
                          {m.differences.map((d) => (
                            <li key={d.rule + d.title}>
                              {d.title}
                              {d.since && <span className={s.since}> · since {formatAgo(d.since)}</span>}
                            </li>
                          ))}
                        </ul>
                      </td>
                      <td>{m.last_sync ? formatAgo(m.last_sync) : 'never'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
      </Panel>
      {data.changes.length > 0 && (
        <Panel title="Setup changes, last 30 days" icon={<History />} flush>
          <ul className={s.changes}>
            {data.changes.map((c, i) => (
              <li key={i}>
                <span className={s.since}>{formatAgo(c.ts)}</span>
                <span className={v.name}>{c.machine}</span>
                <span>
                  {c.change === 'added' ? 'Added' : 'Removed'}: {c.what}
                </span>
              </li>
            ))}
          </ul>
        </Panel>
      )}
    </>
  )
}
