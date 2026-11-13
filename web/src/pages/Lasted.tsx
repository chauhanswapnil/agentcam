import { CalendarRange, FileCode2, FolderGit2, Sparkles, Sprout, X } from 'lucide-react'
import type { ReactNode } from 'react'
import { Link, useLocation, useSearch } from 'wouter'
import type { LastedFile, Lasted as Counts, LastedReport } from '../lib/api'
import { query, useApi, useIndexStatus } from '../lib/api'
import { baseName, formatNumber } from '../lib/format'
import { PARTS, keptPct, pct } from '../lib/lasted'
import { LastedBar, LastedLegend } from '../ui/LastedBar'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import v from './Views.module.css'
import s from './Lasted.module.css'

const RANGES = [
  { value: '30d', label: '30 days' },
  { value: '90d', label: '90 days' },
  { value: 'all', label: 'All time' },
]

const weekFmt = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
const weekLabel = (day: string) => {
  const [y, m, d] = day.split('-').map(Number)
  return `Week of ${weekFmt.format(new Date(y, m - 1, d))}`
}

/** Relative to the project, else with the home folder as `~`. */
const shortPath = (path: string, project: string | null) =>
  project && path.startsWith(project + '/')
    ? path.slice(project.length + 1)
    : path.replace(/^\/(Users|home)\/[^/]+/, '~')

/** One row: a name, the split bar, and the share still in the code. */
function Row({
  label,
  sub,
  c,
  href,
  onPick,
  active,
}: {
  label: ReactNode
  sub?: ReactNode
  c: Counts
  href?: string
  /** Makes the row a button that picks it, to narrow the files table. */
  onPick?: () => void
  active?: boolean
}) {
  const k = keptPct(c)
  const name = (
    <span className={s.rowName}>
      {label}
      {sub && <span className={s.rowSub}>{sub}</span>}
    </span>
  )
  const body = (
    <>
      {href ? (
        <Link href={href} className={s.rowLink}>
          {name}
        </Link>
      ) : (
        name
      )}
      <LastedBar c={c} label={typeof label === 'string' ? label : undefined} />
      <span className={s.rowPct}>{k == null ? '–' : `${Math.round(k)}%`}</span>
      <span className={s.rowLines}>{formatNumber(c.added)} lines</span>
    </>
  )
  return onPick ? (
    <button type="button" className={`${s.row} ${s.pick} ${active ? s.pickOn : ''}`} aria-pressed={active} onClick={onPick} title="Show the files from this">
      {body}
    </button>
  ) : (
    <div className={s.row}>{body}</div>
  )
}

/**
 * Of the lines agents added, how many are still in the code. Read from the
 * diffs in the logs and the files on disk today; see `survival.rs`.
 */
export function Lasted() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const range = p.get('range') ?? '90d'
  // The files table can be narrowed to one project or one week.
  const project = p.get('project') ?? ''
  const week = p.get('week') ?? ''
  const set = (patch: { project?: string; week?: string }) =>
    navigate(`/lasted${query({ range: range === '90d' ? undefined : range, project, week, ...patch })}`, { replace: true })
  const gen = useIndexStatus().status?.generation ?? 0
  const { data } = useApi<LastedReport>(`/api/lasted${query({ range })}`, gen)
  const narrowed = useApi<LastedFile[]>(project || week ? `/api/lasted/files${query({ range, project, week })}` : null, gen)
  const files = project || week ? narrowed.data : (data?.files ?? null)

  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>What lasted</h1>
          <p>
            Lines agents added, checked against your files today. Edits from the last day are still settling, so
            they're counted apart.
          </p>
        </div>
        <div className={v.controls}>
          <Segmented
            label="Edits made in"
            value={range}
            onChange={(r) => navigate(`/lasted${query({ range: r === '90d' ? undefined : r })}`, { replace: true })}
            options={RANGES}
          />
        </div>
      </header>

      {!data ? (
        <Skeleton height={260} />
      ) : data.settled.added === 0 && data.recent.added === 0 ? (
        <Panel>
          <EmptyState icon={<Sprout />} title="No agent edits to check in this range">
            Edits with a diff in the log (Claude Code Edit and Write, Codex patches) show up here once their files can
            be read.
          </EmptyState>
        </Panel>
      ) : (
        <>
          <Hero data={data} />
          <div className={s.grid}>
            <Panel title="By age" icon={<CalendarRange />}>
              <div className={s.rows}>
                {data.ages.map((a) => (
                  <Row key={a.label} label={a.label} c={a.counts} />
                ))}
              </div>
            </Panel>
            <Panel title="By agent and model" icon={<Sparkles />}>
              <div className={s.rows}>
                {data.by_agent.map((a) => (
                  <Row
                    key={`${a.agent}-${a.model}`}
                    label={
                      <span className={s.agentName}>
                        <AgentLabel agent={a.agent} hideName />
                        {a.model ?? 'Unknown model'}
                      </span>
                    }
                    sub={`${a.sessions} session${a.sessions === 1 ? '' : 's'}`}
                    c={a.counts}
                  />
                ))}
              </div>
            </Panel>
          </div>

          <Panel title="By week written" icon={<CalendarRange />}>
            <div className={s.rows}>
              {data.weeks.map((w) => (
                <Row key={w.week} label={weekLabel(w.week)} c={w.counts} active={week === w.week} onPick={() => set({ week: week === w.week ? '' : w.week })} />
              ))}
            </div>
          </Panel>

          <Panel title="By project" icon={<FolderGit2 />}>
            <div className={s.rows}>
              {data.by_project.map((x) => (
                <Row
                  key={x.project ?? ''}
                  label={x.project ? baseName(x.project) : 'No project'}
                  sub={`${x.sessions} session${x.sessions === 1 ? '' : 's'}`}
                  c={x.counts}
                  active={!!x.project && project === x.project}
                  onPick={x.project ? () => set({ project: project === x.project ? '' : x.project! }) : undefined}
                />
              ))}
            </div>
          </Panel>

          {(files === null || files.length > 0 || project || week) && (
            <Panel
              title="Files whose agent lines changed most"
              icon={<FileCode2 />}
              flush
              actions={
                (project || week) && (
                  <span className={s.chips}>
                    {week && (
                      <button type="button" className={s.chip} onClick={() => set({ week: '' })} aria-label={`Remove week filter: ${weekLabel(week)}`}>
                        {weekLabel(week)} <X aria-hidden />
                      </button>
                    )}
                    {project && (
                      <button type="button" className={s.chip} onClick={() => set({ project: '' })} aria-label={`Remove project filter: ${baseName(project)}`}>
                        {baseName(project)} <X aria-hidden />
                      </button>
                    )}
                  </span>
                )
              }
            >
              {files && files.length === 0 && <p className={v.note} style={{ padding: 'var(--s-4)' }}>No files with five or more settled agent lines here.</p>}
              <div className={v.tableWrap}>
                <table className={v.table}>
                  <thead>
                    <tr>
                      <th>File</th>
                      <th className={v.num}>Edits</th>
                      <th className={v.num}>Lines added</th>
                      <th className={s.barCol}>Where they are now</th>
                      <th className={v.num}>Still there</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(files ?? []).map((f) => (
                      <tr key={f.path}>
                        <td className={s.pathCell}>
                          <Link href={`/files${query({ path: f.path, range: 'all' })}`} className={v.path} title={f.path}>
                            {shortPath(f.path, f.project)}
                          </Link>
                        </td>
                        <td className={v.num}>{f.edits}</td>
                        <td className={v.num}>{formatNumber(f.counts.added)}</td>
                        <td className={s.barCol}>
                          <LastedBar c={f.counts} />
                        </td>
                        <td className={v.num}>{pct(f.counts.kept, f.counts.added)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Panel>
          )}

          <p className={s.how}>
            How this works: for each edit with a diff in the log, agentcam takes the lines it added (all lines for a new
            file) and looks for them in the file today, by trimmed text. Lines with fewer than three letters or digits
            (braces, blank lines) are skipped. A line that's gone counts as rewritten by agents when a later agent edit
            removed it, and as changed since otherwise. Moved lines still count as there; reformatted lines don't.
            Files are only read on this machine, and only counts are stored.
          </p>
        </>
      )}
    </div>
  )
}

function Hero({ data }: { data: LastedReport }) {
  const c = data.settled
  const k = keptPct(c)
  return (
    <section className={s.hero} aria-label="Summary">
      <div className={s.heroMain}>
        <span className={s.big}>{k == null ? '–' : `${Math.round(k)}%`}</span>
        <span className={s.bigText}>
          of the lines agents wrote are still in the code
          <span className={s.bigSub}>{formatNumber(c.added)} lines from edits at least a day old</span>
        </span>
      </div>
      <LastedBar c={c} height={14} label="All settled edits" />
      <div className={s.heroParts}>
        {PARTS.map((part) => (
          <div key={part.key} className={s.part} title={part.hint}>
            <span className={s.partLabel}>
              <span className={s.swatch} style={{ background: part.color }} aria-hidden />
              {part.label}
            </span>
            <span className={s.partValue}>{pct(c[part.key], c.added)}</span>
            <span className={s.partSub}>{formatNumber(c[part.key])} lines</span>
          </div>
        ))}
        <div className={s.part}>
          <span className={s.partLabel}>Last day, still settling</span>
          <span className={s.partValue}>{formatNumber(data.recent.added)}</span>
          <span className={s.partSub}>
            lines, {pct(data.recent.kept, data.recent.added)} there so far
          </span>
        </div>
      </div>
      <LastedLegend />
    </section>
  )
}
