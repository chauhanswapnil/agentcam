import { useMemo } from 'react'
import type { EventRow } from '../lib/api'
import { formatCompact } from '../lib/format'
import s from './ContextChart.module.css'

type Props = { events: EventRow[]; current: number; onSeek: (i: number) => void }

const W = 1000
const H = 120
const BOTTOM = 8

/**
 * How full the context window was at each model call. A dashed line marks
 * a compaction. Click anywhere to move the playhead there.
 */
export function ContextChart({ events, current, onSeek }: Props) {
  const pts = useMemo(() => {
    const out: { i: number; v: number }[] = []
    events.forEach((e, i) => {
      if (e.context_tokens != null) out.push({ i, v: e.context_tokens })
    })
    return out
  }, [events])
  const compactions = useMemo(
    () => events.flatMap((e, i) => (e.kind === 'system' && e.title === 'Context compacted' ? [i] : [])),
    [events],
  )
  if (pts.length < 2) return <p className={s.empty}>No token counts in this log.</p>

  const max = Math.max(...pts.map((p) => p.v))
  const n = Math.max(1, events.length - 1)
  const x = (i: number) => (i / n) * W
  const y = (v: number) => H - BOTTOM - (v / max) * (H - BOTTOM - 8)
  const line = pts.map((p, k) => (k ? 'L' : 'M') + x(p.i).toFixed(1) + ',' + y(p.v).toFixed(1)).join(' ')
  const area = line + ' L' + x(pts[pts.length - 1].i) + ',' + (H - BOTTOM) + ' L' + x(pts[0].i) + ',' + (H - BOTTOM) + ' Z'
  const now = [...pts].reverse().find((p) => p.i <= current)

  return (
    <div>
      <svg
        className={s.svg}
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        role="img"
        aria-label={`Context window per model call, peak ${formatCompact(max)} tokens`}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect()
          onSeek(Math.round(((e.clientX - r.left) / r.width) * n))
        }}
      >
        <path d={area} className={s.area} />
        <path d={line} className={s.line} vectorEffect="non-scaling-stroke" />
        {compactions.map((i) => (
          <line key={i} x1={x(i)} x2={x(i)} y1={4} y2={H - BOTTOM} className={s.compact} vectorEffect="non-scaling-stroke" />
        ))}
        <line x1={x(current)} x2={x(current)} y1={0} y2={H - BOTTOM} className={s.playhead} vectorEffect="non-scaling-stroke" />
      </svg>
      <div className={s.legend}>
        <span>
          At this step: <b>{now ? formatCompact(now.v) : '–'}</b> tokens
        </span>
        {compactions.length > 0 && (
          <span>
            <i className={s.dash} />
            {compactions.length} {compactions.length === 1 ? 'compaction' : 'compactions'}
          </span>
        )}
        <span>Peak {formatCompact(max)}</span>
      </div>
    </div>
  )
}
