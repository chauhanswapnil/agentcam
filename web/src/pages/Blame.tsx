import { FileCode2, GitCommitVertical } from 'lucide-react'
import { useMemo, useState } from 'react'
import { Link, useSearch } from 'wouter'
import { query, useApi, useIndexStatus } from '../lib/api'
import { formatAgo, formatNumber, oneLine } from '../lib/format'
import { AgentLabel, EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import v from './Views.module.css'
import s from './Blame.module.css'

/** Mirrors `Blame` in agentcam-core `survival.rs`. */
type BlameEdit = {
  session_id: string
  seq: number
  ts: string
  agent: 'claude-code' | 'codex'
  model: string | null
  session_title: string | null
  prompt: string | null
  lines: number
}
type Blame = {
  path: string
  found: boolean
  too_big: boolean
  lines: { n: number; text: string; edit: number | null }[]
  edits: BlameEdit[]
  agent_lines: number
}

const short = (p: string) => p.replace(/^\/(Users|home)\/[^/]+/, '~')

/**
 * Which agent session wrote each line of a file as it is today. Read from
 * the diffs in the logs and the file on disk; nothing is stored.
 */
export function BlamePage() {
  const path = new URLSearchParams(useSearch()).get('path') ?? ''
  const gen = useIndexStatus().status?.generation ?? 0
  const { data, error } = useApi<Blame>(path ? `/api/blame${query({ path })}` : null, gen)
  const [hi, setHi] = useState<number | null>(null)
  // Runs of lines from one edit get one label, on their first line.
  const starts = useMemo(() => {
    const m = new Set<number>()
    data?.lines.forEach((l, i) => {
      if (l.edit != null && data.lines[i - 1]?.edit !== l.edit) m.add(i)
    })
    return m
  }, [data])

  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Who wrote this</h1>
          <p className={s.path} title={path}>
            {short(path) || 'Pick a file on the Files page.'}
          </p>
        </div>
        {path && (
          <div className={v.controls}>
            <Link href={`/files${query({ path, range: 'all' })}`} className={s.back}>
              File history
            </Link>
          </div>
        )}
      </header>
      {!path ? (
        <Panel>
          <EmptyState icon={<FileCode2 />} title="No file picked" />
        </Panel>
      ) : error ? (
        <p className={v.note}>{error}</p>
      ) : !data ? (
        <Skeleton height={400} />
      ) : !data.found || data.too_big ? (
        <Panel>
          <EmptyState icon={<FileCode2 />} title={data.too_big ? 'This file is too big to check' : "This file isn't on disk any more"} />
        </Panel>
      ) : data.lines.length === 0 ? (
        <Panel>
          <EmptyState icon={<FileCode2 />} title="No agent session edited this file" />
        </Panel>
      ) : (
        <div className={s.layout}>
          <Panel title={`${formatNumber(data.agent_lines)} of ${formatNumber(data.lines.length)} lines from agent sessions`} icon={<GitCommitVertical />} flush>
            {data.edits.length === 0 ? (
              <EmptyState title="No agent edit wrote a line that's still here" />
            ) : (
              <ul className={s.edits}>
                {[...data.edits.keys()].reverse().map((i) => {
                  const e = data.edits[i]
                  return (
                    <li key={i}>
                      <button
                        type="button"
                        className={`${s.edit} ${hi === i ? s.editOn : ''}`}
                        onMouseEnter={() => setHi(i)}
                        onMouseLeave={() => setHi(null)}
                        onFocus={() => setHi(i)}
                        onBlur={() => setHi(null)}
                        onClick={() => document.getElementById(`blame-${i}`)?.scrollIntoView({ block: 'center' })}
                      >
                        <span className={s.editTop}>
                          <AgentLabel agent={e.agent} hideName />
                          <span className={s.model}>{e.model ?? 'Unknown model'}</span>
                          <span className={s.when}>{formatAgo(e.ts)}</span>
                        </span>
                        <span className={s.prompt}>{oneLine(e.prompt ?? e.session_title ?? '')}</span>
                        <span className={s.count}>{formatNumber(e.lines)} line{e.lines === 1 ? "" : "s"}</span>
                      </button>
                      <Link href={`/sessions/${e.session_id}?step=${e.seq}`} className={s.open}>
                        Open step {e.seq + 1}
                      </Link>
                    </li>
                  )
                })}
              </ul>
            )}
          </Panel>
          <Panel flush className={s.codePanel}>
            <div className={s.code} role="table" aria-label="File lines and who wrote them">
              {data.lines.map((l, i) => {
                const e = l.edit != null ? data.edits[l.edit] : null
                return (
                  <div
                    key={l.n}
                    id={starts.has(i) ? `blame-${l.edit}` : undefined}
                    role="row"
                    className={`${s.line} ${e ? s.agentLine : ''} ${hi != null && l.edit === hi ? s.lineHi : ''}`}
                  >
                    <span role="cell" className={s.who}>
                      {e && starts.has(i) && (
                        <Link href={`/sessions/${e.session_id}?step=${e.seq}`} className={s.whoLink} title={oneLine(e.prompt ?? '')}>
                          {(e.model ?? e.agent).replace(/^claude-/, '')} · {formatAgo(e.ts)}
                        </Link>
                      )}
                    </span>
                    <span role="cell" className={s.n}>
                      {l.n}
                    </span>
                    <span role="cell" className={s.text}>
                      {l.text || ' '}
                    </span>
                  </div>
                )
              })}
            </div>
          </Panel>
        </div>
      )}
      <p className={v.note}>
        A line belongs to the latest agent edit that added the same text (ignoring indentation). Lines nobody's agent
        wrote, or that changed since, have no label. Blank lines and braces inside one edit's block count with it.
      </p>
    </div>
  )
}
