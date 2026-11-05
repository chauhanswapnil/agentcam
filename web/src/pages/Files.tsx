import { FileStack } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useLocation, useSearch } from 'wouter'
import { AGENTS } from '../lib/agents'
import { query, useApi, useIndexStatus, type FileStat, type Overview, type StepRef } from '../lib/api'
import { baseName, formatAgo, formatNumber, oneLine } from '../lib/format'
import { EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import s from './Views.module.css'

const RANGES = [
  { value: '7d', label: '7 days' },
  { value: '30d', label: '30 days' },
  { value: 'all', label: 'All time' },
]
const ACTS = [
  { key: 'creates', label: 'new', color: 'var(--status-good)' },
  { key: 'edits', label: 'edited', color: 'var(--kind-change)' },
  { key: 'deletes', label: 'deleted', color: 'var(--status-critical)' },
  { key: 'reads', label: 'read', color: 'var(--kind-look)' },
] as const

/** Files the agents touched, most changed first; pick one for its history. */
export function Files() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const f = { range: p.get('range') ?? '30d', project: p.get('project') ?? '', path: p.get('path') ?? '' }
  const set = (patch: Partial<typeof f>) => navigate(`/files${query({ ...f, ...patch })}`, { replace: true })
  const [text, setText] = useState('')
  const [changedOnly, setChangedOnly] = useState('changed')
  const gen = useIndexStatus().status?.generation ?? 0
  const projects = useApi<Overview>(`/api/overview${query({ range: f.range })}`, gen).data?.projects ?? []
  const { data } = useApi<FileStat[]>(`/api/files${query({ range: f.range, project: f.project, limit: 2000 })}`, gen)
  const hist = useApi<StepRef[]>(f.path ? `/api/file${query({ path: f.path })}` : null, gen)

  const rows = useMemo(() => {
    const needle = text.trim().toLowerCase()
    return (data ?? []).filter(
      (x) => (changedOnly === 'all' || x.edits + x.creates + x.deletes > 0) && (!needle || x.path.toLowerCase().includes(needle)),
    )
  }, [data, text, changedOnly])
  const rel = (path: string) => (f.project && path.startsWith(f.project + '/') ? path.slice(f.project.length + 1) : path)

  return (
    <div className={s.page}>
      <header className={s.head}>
        <div>
          <h1>Files</h1>
          <p>Every file your agents read or changed, most changed first. Pick one to see each step that touched it.</p>
        </div>
        <div className={s.controls}>
          <select className={s.select} value={f.project} onChange={(e) => set({ project: e.target.value, path: '' })} aria-label="Project">
            <option value="">All projects</option>
            {projects
              .filter((x) => x.path)
              .map((x) => (
                <option key={x.path} value={x.path}>
                  {baseName(x.path)}
                </option>
              ))}
          </select>
          <input className={s.filter} placeholder="Filter paths…" value={text} onChange={(e) => setText(e.target.value)} aria-label="Filter paths" />
          <Segmented
            label="Which files"
            value={changedOnly}
            onChange={setChangedOnly}
            options={[
              { value: 'changed', label: 'Changed' },
              { value: 'all', label: 'All' },
            ]}
          />
          <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
        </div>
      </header>
      {!data ? (
        <Skeleton height={300} />
      ) : (
        <div className={s.split}>
          <Panel title={`${formatNumber(rows.length)} files`} icon={<FileStack />} flush>
            {rows.length === 0 ? (
              <EmptyState title="No files match" />
            ) : (
              <div className={`${s.tableWrap} ${s.list}`}>
                <table className={s.table}>
                  <thead>
                    <tr>
                      <th>Path</th>
                      <th>Actions</th>
                      <th className={s.num}>Sessions</th>
                      <th>Last</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.slice(0, 500).map((x) => (
                      <tr key={x.path} className={`${s.rowBtn} ${f.path === x.path ? s.rowOn : ''}`} onClick={() => set({ path: f.path === x.path ? '' : x.path })}>
                        <td>
                          <div className={s.path} title={x.path}>
                            <bdi>{rel(x.path)}</bdi>
                          </div>
                        </td>
                        <td>
                          <span className={s.acts}>
                            {ACTS.filter((a) => x[a.key] > 0).map((a) => (
                              <span key={a.key} className={s.act} title={`${x[a.key]} ${a.label}`}>
                                <i style={{ background: a.color }} />
                                {x[a.key]} {a.label}
                              </span>
                            ))}
                          </span>
                        </td>
                        <td className={s.num}>{x.sessions}</td>
                        <td>{x.last_ts ? formatAgo(x.last_ts) : ''}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Panel>
          <div className={s.sticky}>
            <Panel title={f.path ? baseName(f.path) : 'History'} flush>
              {!f.path ? (
                <EmptyState title="Pick a file">Each step that read or changed it shows here.</EmptyState>
              ) : (
                <div className={s.list}>
                  {(hist.data ?? []).map((h) => (
                    <button key={`${h.session_id}:${h.seq}:${h.action}`} type="button" className={s.ref} onClick={() => navigate(`/sessions/${h.session_id}?step=${h.seq}`)}>
                      <span className={s.refTop}>
                        <span className={s.act}>
                          <i style={{ background: ACTS.find((a) => a.label.startsWith(h.action === 'create' ? 'new' : (h.action ?? '')))?.color ?? 'var(--text-3)' }} />
                          {h.action}
                        </span>
                        <span className={s.refTitle}>{h.title}</span>
                      </span>
                      <span className={s.refMeta}>
                        {AGENTS[h.agent]?.label} · {h.ts ? formatAgo(h.ts) : ''} · {oneLine(h.session_title ?? '')}
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </Panel>
          </div>
        </div>
      )}
    </div>
  )
}
