import { BookOpen, Sparkles } from 'lucide-react'
import { useState } from 'react'
import { Link, useLocation, useSearch } from 'wouter'
import type { SkillRow, SkillsReport } from '../lib/api'
import { query, useApi, useIndexStatus } from '../lib/api'
import { baseName, formatAgo, formatNumber, oneLine } from '../lib/format'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import v from './Views.module.css'
import s from './Skills.module.css'

const RANGES = [
  { value: '30d', label: '30 days' },
  { value: '90d', label: '90 days' },
  { value: 'all', label: 'All time' },
]
const SOURCE: Record<string, string> = {
  'claude-user': 'Your Claude skills',
  'claude-plugin': 'Claude plugin',
  'claude-project': 'Project',
  codex: 'Codex skills',
}
const TRIGGER: Record<string, string> = { agent: 'agent ran it', you: 'you ran it', read: 'agent read it' }
/** About 4 characters per token, for a rough sense of context size. */
const tokens = (chars: number) => `≈${formatNumber(Math.round(chars / 4 / 100) * 100)} tokens`
const used = (k: SkillRow) => Object.values(k.uses).reduce((a, b) => a + b, 0)

/** Which skills exist, which get used, and what they cost in context. */
export function Skills() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const f = { range: p.get('range') ?? '90d', show: p.get('show') ?? 'used' }
  const set = (patch: Partial<typeof f>) => navigate(`/skills${query({ ...f, ...patch })}`, { replace: true })
  const gen = useIndexStatus().status?.generation ?? 0
  const { data } = useApi<SkillsReport>(`/api/skills${query({ range: f.range })}`, gen)
  const [open, setOpen] = useState<string | null>(null)

  const usedSkills = data?.skills.filter((k) => used(k) > 0 || k.steps > 0) ?? []
  const idle = data?.skills.filter((k) => used(k) === 0 && k.steps === 0) ?? []
  const rows = f.show === 'all' ? (data?.skills ?? []) : usedSkills

  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Skills</h1>
          <p>
            Skills the agents ran or read, the ones you ran as slash commands, and the ones that sat unused. Read from the
            logs and your skill folders; only names, descriptions and sizes.
          </p>
        </div>
        <div className={v.controls}>
          <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
        </div>
      </header>

      {!data ? (
        <Skeleton height={300} />
      ) : data.skills.length === 0 ? (
        <Panel>
          <EmptyState icon={<Sparkles />} title="No skills found">
            Skills show up here once an agent lists, runs or reads one, or when they're installed in your skill folders.
          </EmptyState>
        </Panel>
      ) : (
        <>
          <section className={s.stats} aria-label="Skill totals">
            <div>
              <span>Used</span>
              <b>{formatNumber(usedSkills.length)}</b>
              <small>of {formatNumber(data.skills.length)} known</small>
            </div>
            {data.listing_sessions > 0 && (
              <>
                <div>
                  <span>Listed per session</span>
                  <b>{Math.round(data.avg_listed)}</b>
                  <small>by Claude Code, in {formatNumber(data.listing_sessions)} sessions</small>
                </div>
                <div title="Claude Code adds every skill's name and description to the context at the start of a session">
                  <span>Context the listing takes</span>
                  <b>{tokens(data.avg_listing_chars)}</b>
                  <small>each session, before any skill runs</small>
                </div>
              </>
            )}
            <div>
              <span>Not used in this range</span>
              <b>{formatNumber(idle.length)}</b>
              <small>listed or installed</small>
            </div>
          </section>

          <Panel
            title="Skills"
            icon={<Sparkles />}
            flush
            actions={
              <Segmented
                label="Show"
                value={f.show}
                onChange={(show) => set({ show })}
                options={[
                  { value: 'used', label: 'Used' },
                  { value: 'all', label: 'All' },
                ]}
              />
            }
          >
            <div className={v.tableWrap}>
              <table className={v.table}>
                <thead>
                  <tr>
                    <th>Skill</th>
                    <th className={v.num}>Runs</th>
                    <th className={v.num}>Steps under it</th>
                    <th className={v.num}>Sessions</th>
                    <th className={v.num}>Size when loaded</th>
                    <th>Last used</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((k) => (
                    <tr key={k.name} className={v.rowBtn} onClick={() => setOpen(open === k.name ? null : k.name)}>
                      <td className={s.nameCell}>
                        <span className={s.name}>{k.name}</span>
                        <span className={s.sub}>
                          {[
                            ...k.sources.map((x) => SOURCE[x] ?? x),
                            k.listed_in ? `listed in ${formatNumber(k.listed_in)} sessions` : null,
                          ]
                            .filter(Boolean)
                            .join(' · ') || 'Seen in logs'}
                        </span>
                        {open === k.name && k.description && <span className={s.desc}>{k.description}</span>}
                      </td>
                      <td className={v.num} title={Object.entries(k.uses).map(([t, n]) => `${n} ${TRIGGER[t] ?? t}`).join(', ')}>
                        {used(k) ? formatNumber(used(k)) : '–'}
                      </td>
                      <td className={v.num}>
                        {k.steps ? formatNumber(k.steps) : '–'}
                        {k.errored_steps > 0 && <span className={s.sub}> · {k.errored_steps} errored</span>}
                      </td>
                      <td className={v.num}>{k.sessions ? formatNumber(k.sessions) : '–'}</td>
                      <td className={v.num}>
                        {k.avg_loaded_chars ? tokens(k.avg_loaded_chars) : k.file_chars ? tokens(k.file_chars) : '–'}
                      </td>
                      <td>{k.last_used ? formatAgo(k.last_used) : 'never'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>

          {data.recent.length > 0 && (
            <Panel title="Latest runs" icon={<BookOpen />} flush>
              <ul className={s.recent}>
                {data.recent.map((u) => (
                  <li key={`${u.session_id}-${u.seq}-${u.skill}`}>
                    <Link href={`/sessions/${u.session_id}?step=${u.seq}`} className={s.use}>
                      <AgentLabel agent={u.agent} hideName />
                      <span className={s.useSkill}>{u.skill}</span>
                      <span className={s.useWho}>{TRIGGER[u.trigger] ?? u.trigger}</span>
                      <span className={s.useArgs}>{u.args ? oneLine(u.args) : oneLine(u.title ?? '')}</span>
                      <span className={s.useWhen}>
                        {u.project_root ? `${baseName(u.project_root)} · ` : ''}
                        {u.ts ? formatAgo(u.ts) : ''}
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            </Panel>
          )}
        </>
      )}
    </div>
  )
}
