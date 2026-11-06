import { useRef, useState } from 'react'
import { formatTime, dayLabel, localDay } from '../lib/format'
import s from './UsageChart.module.css'

const DAY = 86_400_000
const H = 56

type Point = { ts: string; used_percent: number }

/** A usage window's readings over the last 7 days, 0–100%. */
export function UsageChart({
  points,
  now,
  label,
  breakMs = 5 * 3_600_000,
}: {
  points: Point[]
  now: number
  label: string
  /** Readings further apart than this aren't joined: nothing ran between them. */
  breakMs?: number
}) {
  const ref = useRef<HTMLDivElement>(null)
  const [hover, setHover] = useState<number | null>(null)
  const start = now - 7 * DAY
  const xs = points.map((p) => Math.max(0, Math.min(1, (new Date(p.ts).getTime() - start) / (7 * DAY))))
  const y = (v: number) => H - (Math.min(100, v) / 100) * (H - 4) - 2
  const times = points.map((p) => new Date(p.ts).getTime())
  const runs: number[][] = []
  points.forEach((_, i) => {
    if (i === 0 || times[i] - times[i - 1] > breakMs) runs.push([i])
    else runs[runs.length - 1].push(i)
  })
  const at = (i: number) => `${(xs[i] * 100).toFixed(2)},${y(points[i].used_percent).toFixed(1)}`
  // A lone reading gets a short tick so it stays visible.
  const line = runs
    .map((r) => (r.length === 1 ? `M${at(r[0])} h0.6` : r.map((i, k) => `${k ? 'L' : 'M'}${at(i)}`).join(' ')))
    .join(' ')
  const area = runs
    .filter((r) => r.length > 1)
    .map((r) => `${r.map((i, k) => `${k ? 'L' : 'M'}${at(i)}`).join(' ')} L${(xs[r[r.length - 1]] * 100).toFixed(2)},${H} L${(xs[r[0]] * 100).toFixed(2)},${H} Z`)
    .join(' ')

  const onMove = (e: React.MouseEvent) => {
    const r = ref.current?.getBoundingClientRect()
    if (!r) return
    const f = (e.clientX - r.left) / r.width
    let best = 0
    for (let i = 1; i < xs.length; i++) if (Math.abs(xs[i] - f) < Math.abs(xs[best] - f)) best = i
    setHover(best)
  }
  const hp = hover != null ? points[hover] : null

  return (
    <div className={s.wrap}>
      <div
        ref={ref}
        className={s.plot}
        onMouseMove={onMove}
        onMouseLeave={() => setHover(null)}
        role="img"
        aria-label={`${label} use over the last 7 days, ${points.length} readings`}
      >
        <svg viewBox={`0 0 100 ${H}`} preserveAspectRatio="none" className={s.svg} aria-hidden>
          <line x1="0" x2="100" y1={y(75)} y2={y(75)} className={s.guide} vectorEffect="non-scaling-stroke" />
          <path d={area} className={s.area} />
          <path d={line} className={s.line} vectorEffect="non-scaling-stroke" />
          {hover != null && (
            <line x1={xs[hover] * 100} x2={xs[hover] * 100} y1="0" y2={H} className={s.cross} vectorEffect="non-scaling-stroke" />
          )}
        </svg>
        {hp && hover != null && (
          <>
            <span className={s.marker} style={{ left: `${xs[hover] * 100}%`, top: y(hp.used_percent) }} />
            <span className={s.tip} style={{ left: `${Math.min(80, Math.max(20, xs[hover] * 100))}%` }}>
              <b>{Math.round(hp.used_percent)}%</b> {dayLabel(localDay(new Date(hp.ts)))}, {formatTime(hp.ts)}
            </span>
          </>
        )}
      </div>
      <div className={s.axis}>
        <span>7 days ago</span>
        <span>now</span>
      </div>
    </div>
  )
}
