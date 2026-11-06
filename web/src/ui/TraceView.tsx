import { ChevronDown, ChevronRight } from 'lucide-react'
import { memo, useEffect, useMemo, useRef, useState } from 'react'
import { useApi, type EventRow, type SessionDetail, type SessionRow } from '../lib/api'
import { formatDuration } from '../lib/format'
import { GROUPS, groupOf, kindLabel } from '../lib/kinds'
import { KindIcon } from './Misc'
import s from './TraceView.module.css'

/** Gaps longer than this are idle time, not agent time (same rule as the index). */
const IDLE_CAP = 5 * 60_000

/** Each step's start on an agent-time axis: idle stretches squeezed out. */
function layout(events: EventRow[]) {
  const starts: number[] = []
  let t = 0
  let prev: number | null = null
  for (const e of events) {
    const ts = e.ts ? Date.parse(e.ts) : null
    if (prev != null && ts != null) t += Math.min(Math.max(ts - prev, 0), IDLE_CAP)
    if (ts != null) prev = ts
    starts.push(t)
  }
  const last = events.length - 1
  const total = Math.max(1, (starts[last] ?? 0) + (events[last]?.duration_ms ?? 0))
  return { starts, total }
}

type Props = {
  events: EventRow[]
  subagents: SessionRow[]
  current: number
  onSeek: (i: number) => void
}

/**
 * The session as a trace: one row per step on an agent-time axis, with
 * tool calls drawn as spans as long as they ran. Subagent calls expand to
 * show the subagent's own steps inside the parent span.
 */
export function TraceView({ events, subagents, current, onSeek }: Props) {
  const { starts, total } = useMemo(() => layout(events), [events])
  // Subagent tool calls, in order, pair with the subagent sessions in order.
  const childFor = useMemo(() => {
    const calls = events.flatMap((e, i) => (e.category === 'subagent' ? [i] : []))
    const sorted = [...subagents].sort((a, b) => (a.started_at ?? '').localeCompare(b.started_at ?? ''))
    return new Map(calls.map((i, k) => [i, sorted[k]] as const).filter(([, c]) => c))
  }, [events, subagents])
  const ticks = [0, 0.25, 0.5, 0.75, 1]
  const box = useRef<HTMLDivElement>(null)
  // Follow the playhead inside the list, never scrolling the page.
  useEffect(() => {
    const b = box.current
    const el = b?.querySelector<HTMLElement>(`[data-i="${current}"]`)
    if (!b || !el) return
    if (el.offsetTop < b.scrollTop + 30 || el.offsetTop > b.scrollTop + b.clientHeight - 30) {
      b.scrollTo({ top: el.offsetTop - b.clientHeight / 3 })
    }
  }, [current])

  return (
    <div className={s.trace}>
      <div className={s.axis}>
        <span className={s.labelCol}>Step</span>
        <span className={s.barCol}>
          {ticks.map((t) => (
            <i key={t} style={{ left: `${t * 100}%` }}>
              {formatDuration(t * total)}
            </i>
          ))}
        </span>
      </div>
      <div className={s.rows} ref={box}>
        {events.map((e, i) => (
          <Row
            key={e.seq}
            e={e}
            i={i}
            on={i === current}
            left={starts[i] / total}
            width={(e.duration_ms ?? 0) / total}
            child={childFor.get(i)}
            onSeek={onSeek}
          />
        ))}
      </div>
    </div>
  )
}

const Row = memo(function Row({
  e,
  i,
  on,
  left,
  width,
  child,
  onSeek,
}: {
  e: EventRow
  i: number
  on: boolean
  left: number
  width: number
  child?: SessionRow
  onSeek: (i: number) => void
}) {
  const [open, setOpen] = useState(false)
  const color = GROUPS[groupOf(e.kind, e.category, e.is_error)].color
  const span = e.kind === 'tool_call' && width > 0
  return (
    <>
      <div data-i={i} className={`${s.row} ${on ? s.on : ''} ${e.wasted ? s.wasted : ''}`} onClick={() => onSeek(i)}>
        <span className={s.labelCol}>
          {child ? (
            <button
              type="button"
              className={s.toggle}
              aria-label={open ? 'Hide subagent steps' : 'Show subagent steps'}
              onClick={(ev) => {
                ev.stopPropagation()
                setOpen(!open)
              }}
            >
              {open ? <ChevronDown aria-hidden /> : <ChevronRight aria-hidden />}
            </button>
          ) : (
            <span className={s.toggleSpace} />
          )}
          <KindIcon kind={e.kind} category={e.category} isError={e.is_error} />
          <span className={s.title} title={e.title}>
            {e.kind === 'tool_call' ? e.title : kindLabel(e.kind, e.category)}
          </span>
        </span>
        <span className={s.barCol}>
          {span ? (
            <i
              className={s.span}
              style={{ left: `${left * 100}%`, width: `max(3px, ${width * 100}%)`, background: color }}
              title={`${kindLabel(e.kind, e.category)} · ${formatDuration(e.duration_ms ?? 0)}`}
            />
          ) : (
            <i className={s.dot} style={{ left: `${left * 100}%`, background: color }} />
          )}
        </span>
      </div>
      {open && child && <ChildRows id={child.id} left={left} width={Math.max(width, 0.002)} />}
    </>
  )
})

/** A subagent's steps, drawn inside the parent call's span. */
function ChildRows({ id, left, width }: { id: string; left: number; width: number }) {
  const { data } = useApi<SessionDetail>(`/api/sessions/${id}`)
  const lay = useMemo(() => (data ? layout(data.events) : null), [data])
  if (!data || !lay) return <div className={`${s.row} ${s.child}`}>Loading subagent…</div>
  return (
    <>
      {data.events.map((e, i) => {
        const color = GROUPS[groupOf(e.kind, e.category, e.is_error)].color
        const l = left + (lay.starts[i] / lay.total) * width
        const w = ((e.duration_ms ?? 0) / lay.total) * width
        return (
          <div key={e.seq} className={`${s.row} ${s.child}`}>
            <span className={s.labelCol}>
              <span className={s.toggleSpace} />
              <span className={s.indent} />
              <KindIcon kind={e.kind} category={e.category} isError={e.is_error} />
              <span className={s.title} title={e.title}>
                {e.kind === 'tool_call' ? e.title : kindLabel(e.kind, e.category)}
              </span>
            </span>
            <span className={s.barCol}>
              {e.kind === 'tool_call' && w > 0 ? (
                <i className={s.span} style={{ left: `${l * 100}%`, width: `max(2px, ${w * 100}%)`, background: color }} />
              ) : (
                <i className={s.dot} style={{ left: `${l * 100}%`, background: color }} />
              )}
            </span>
          </div>
        )
      })}
    </>
  )
}
