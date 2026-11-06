import { ScrollText } from 'lucide-react'
import { Link, useLocation, useSearch } from 'wouter'
import type { ProjectFollow } from '../lib/api'
import { query, useApi, useIndexStatus } from '../lib/api'
import { baseName, formatAgo, formatNumber, oneLine } from '../lib/format'
import { ruleText } from '../lib/follow'
import { CodeText } from '../ui/Code'
import { EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import v from './Views.module.css'
import s from './Instructions.module.css'

const RANGES = [
  { value: '7d', label: '7 days' },
  { value: '30d', label: '30 days' },
  { value: 'all', label: 'All time' },
]

/** What each project's AGENTS.md or CLAUDE.md asks, and how often sessions did it. */
export function Instructions() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const range = p.get('range') ?? '30d'
  const gen = useIndexStatus().status?.generation ?? 0
  const { data } = useApi<ProjectFollow[]>(`/api/follow${query({ range })}`, gen)

  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Instructions</h1>
          <p>
            Rules from each project's AGENTS.md and CLAUDE.md that the logs can check: commands to run after editing, the
            package manager, files not to edit, commands not to run. Checked against the files as they are today.
          </p>
        </div>
        <div className={v.controls}>
          <Segmented label="Time range" value={range} onChange={(r) => navigate(`/instructions${query({ range: r })}`, { replace: true })} options={RANGES} />
        </div>
      </header>

      {!data ? (
        <Skeleton height={260} />
      ) : data.length === 0 ? (
        <Panel>
          <EmptyState icon={<ScrollText />} title="No checkable instructions found">
            Projects with an AGENTS.md or CLAUDE.md that lists commands to run before finishing, or files and commands to
            avoid, show up here.
          </EmptyState>
        </Panel>
      ) : (
        data.map((proj) => (
          <Panel key={proj.project} title={`${baseName(proj.project)} · ${proj.sessions} sessions`} icon={<ScrollText />} flush>
            <p className={s.files}>{proj.files.map((f) => f.slice(proj.project.length + 1) || f).join(' · ')}</p>
            <ul className={s.rules}>
              {proj.rules.map((r) => {
                const checked = r.followed + r.not_followed
                return (
                  <li key={r.rule.id} className={s.rule}>
                    <div className={s.ruleTop}>
                      <span className={s.ruleText}>
                        <CodeText text={ruleText(r.rule)} />
                      </span>
                      <span className={s.count}>
                        {checked ? (
                          <>
                            <b>{formatNumber(r.followed)}</b> of {formatNumber(checked)} sessions
                          </>
                        ) : (
                          'no session it applied to'
                        )}
                      </span>
                    </div>
                    {checked > 0 && (
                      <div className={s.track} role="img" aria-label={`${r.followed} of ${checked} sessions followed it`}>
                        <span style={{ width: `${(100 * r.followed) / checked}%` }} />
                      </div>
                    )}
                    <p className={s.source}>
                      From {r.rule.file.split('/').pop()}: <CodeText text={oneLine(r.rule.source)} />
                    </p>
                    {r.examples.length > 0 && (
                      <ul className={s.examples}>
                        {r.examples.slice(0, 4).map((x) => (
                          <li key={`${x.session_id}-${x.seq}`}>
                            <Link href={`/sessions/${x.session_id}${x.seq != null ? `?step=${x.seq}` : ''}`} className={s.example}>
                              <span className={s.exTitle}>{oneLine(x.title ?? 'Untitled session')}</span>
                              <span className={s.exDetail}>
                                <CodeText text={x.detail ?? ''} />
                              </span>
                              <span className={s.exWhen}>{x.ts ? formatAgo(x.ts) : ''}</span>
                            </Link>
                          </li>
                        ))}
                      </ul>
                    )}
                  </li>
                )
              })}
            </ul>
          </Panel>
        ))
      )}
    </div>
  )
}
