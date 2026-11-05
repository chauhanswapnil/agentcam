import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import type { Day } from '../lib/api'
import { formatHours, formatNumber, fromLocalDay, localDay } from '../lib/format'
import s from './Heatmap.module.css'

const CELL = 12
const GAP = 3
const STEP = CELL + GAP
const LEFT = 30
const TOP = 18
const WEEKS = 53
const LEVELS = 5

const fullDate = (day: string) =>
  fromLocalDay(day).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric', year: 'numeric' })

type Props = {
  days: Day[]
  selected?: string
  onSelect: (day: string | undefined) => void
}

type Cell = { day: string; col: number; row: number; data?: Day; level: number }

/** Level 1..5 by where `v` sits among the busy days, 0 for none. */
function levels(values: number[]) {
  const sorted = values.filter((v) => v > 0).sort((a, b) => a - b)
  const cut = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0
  const bounds = [0.2, 0.4, 0.6, 0.8].map(cut)
  return (v: number) => (v <= 0 ? 0 : 1 + bounds.filter((b) => v > b).length)
}

/**
 * A year of agent activity, one square per local day, Monday on top.
 * Color is agent time on a single blue ramp. Arrow keys move, Enter picks.
 */
export function Heatmap({ days, selected, onSelect }: Props) {
  const ref = useRef<HTMLDivElement>(null)
  const [hover, setHoverState] = useState<{ cell: Cell; scroll: number } | null>(null)
  const [focus, setFocus] = useState<number | null>(null)
  const [today] = useState(() => localDay(new Date()))
  const setHover = (cell: Cell | null) =>
    setHoverState(cell ? { cell, scroll: ref.current?.scrollLeft ?? 0 } : null)

  const { cells, months, total, activeDays } = useMemo(() => {
    const byDay = new Map(days.map((d) => [d.day, d]))
    const level = levels(days.map((d) => d.active_ms))
    const end = fromLocalDay(today)
    const monday = (end.getDay() + 6) % 7 // days since Monday
    const start = new Date(end)
    start.setDate(start.getDate() - monday - (WEEKS - 1) * 7)

    const cells: Cell[] = []
    const months: { col: number; label: string }[] = []
    let lastMonth = -1
    for (let i = 0; ; i++) {
      const d = new Date(start)
      d.setDate(start.getDate() + i)
      if (d > end) break
      const key = localDay(d)
      const col = Math.floor(i / 7)
      const data = byDay.get(key)
      cells.push({ day: key, col, row: i % 7, data, level: level(data?.active_ms ?? 0) })
      if (i % 7 === 0 && d.getMonth() !== lastMonth) {
        lastMonth = d.getMonth()
        if (col < WEEKS - 1) months.push({ col, label: d.toLocaleString(undefined, { month: 'short' }) })
      }
    }
    const total = days.reduce((n, d) => n + d.active_ms, 0)
    return { cells, months, total, activeDays: days.filter((d) => d.active_ms > 0).length }
  }, [days, today])

  // On narrow screens the grid scrolls; start at the most recent weeks.
  useEffect(() => {
    const el = ref.current
    if (el) el.scrollLeft = el.scrollWidth
  }, [cells.length])

  const width = LEFT + WEEKS * STEP
  const height = TOP + 7 * STEP
  const pos = (c: Cell) => ({ x: LEFT + c.col * STEP, y: TOP + c.row * STEP })

  const onKey = (e: KeyboardEvent) => {
    const moves: Record<string, number> = { ArrowLeft: -7, ArrowRight: 7, ArrowUp: -1, ArrowDown: 1 }
    const cur = focus ?? cells.length - 1
    if (e.key in moves) {
      e.preventDefault()
      const next = Math.max(0, Math.min(cells.length - 1, cur + moves[e.key]))
      setFocus(next)
      setHover(cells[next])
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      const day = cells[cur].day
      onSelect(day === selected ? undefined : day)
    } else if (e.key === 'Escape') {
      onSelect(undefined)
    }
  }

  const tipAt = hover && pos(hover.cell)

  return (
    <div className={s.outer}>
      <div className={s.wrap} ref={ref}>
        <svg
          className={`${s.svg} ${selected ? s.dim : ''}`}
          width={width}
          height={height}
          role="grid"
          aria-label={`Agent activity for the last year: ${formatHours(total)} over ${activeDays} days. Use arrow keys to move between days and Enter to filter.`}
          tabIndex={0}
          onKeyDown={onKey}
          onFocus={() => focus == null && setFocus(cells.length - 1)}
          onBlur={() => setHover(null)}
        >
          {months.map((m) => (
            <text key={m.col} className={s.label} x={LEFT + m.col * STEP} y={10}>
              {m.label}
            </text>
          ))}
          {['Mon', 'Wed', 'Fri'].map((d, i) => (
            <text key={d} className={s.label} x={0} y={TOP + (i * 2) * STEP + CELL - 2}>
              {d}
            </text>
          ))}
          {cells.map((c, i) => {
            const { x, y } = pos(c)
            return (
              <rect
                key={c.day}
                className={`${s.cell} ${c.day === selected ? s.selected : ''} ${i === focus ? s.focused : ''}`}
                x={x}
                y={y}
                width={CELL}
                height={CELL}
                rx={2}
                fill={`var(--heat-${c.level})`}
                onMouseEnter={() => setHover(c)}
                onMouseLeave={() => setHover(null)}
                onClick={() => onSelect(c.day === selected ? undefined : c.day)}
              />
            )
          })}
        </svg>
      </div>
      {hover && tipAt && (
        <div className={s.tip} style={{ left: tipAt.x + CELL / 2 - hover.scroll, top: tipAt.y }}>
          <strong>{fullDate(hover.cell.day)}</strong>
          {hover.cell.data ? (
            <>
              <span>{formatHours(hover.cell.data.active_ms)} of agent time</span>
              <span>
                {hover.cell.data.sessions} {hover.cell.data.sessions === 1 ? 'session' : 'sessions'} ·{' '}
                {formatNumber(hover.cell.data.tool_calls)} tool calls
              </span>
            </>
          ) : (
            <span>No activity</span>
          )}
        </div>
      )}
      <div className={s.foot}>
        <span>
          {formatHours(total)} of agent time over {activeDays} {activeDays === 1 ? 'day' : 'days'}
        </span>
        <span className={s.legend} aria-hidden>
          <span>Less</span>
          {Array.from({ length: LEVELS + 1 }, (_, i) => (
            <i key={i} style={{ background: `var(--heat-${i})` }} />
          ))}
          <span>More</span>
        </span>
      </div>
    </div>
  )
}
