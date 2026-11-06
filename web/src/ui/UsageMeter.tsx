import { OctagonAlert, TriangleAlert } from 'lucide-react'
import type { LimitWindow } from '../lib/api'
import { AGENTS } from '../lib/agents'
import { formatAgo, formatDate, formatTime } from '../lib/format'
import { untilLabel, usageLevel, windowLabel } from '../lib/live'
import { UsageChart } from './UsageChart'
import s from './UsageMeter.module.css'

const LEVEL = {
  ok: null,
  near: { label: 'Near limit', color: 'var(--status-warning)', icon: TriangleAlert },
  reached: { label: 'Limit reached', color: 'var(--status-critical)', icon: OctagonAlert },
} as const


type Props = {
  w: LimitWindow
  points?: { ts: string; used_percent: number }[]
  now: number
}

/** How full one usage window is, from the agent's latest reading. */
export function UsageMeter({ w, points, now }: Props) {
  const pct = w.expired ? 0 : Math.min(100, w.used_percent)
  const level = w.expired ? null : LEVEL[usageLevel(w.used_percent)]
  const resetMs = w.resets_at ? new Date(w.resets_at).getTime() - now : null
  return (
    <div className={s.card}>
      <div className={s.top}>
        <span className={s.name}>
          <span className={s.dot} style={{ background: AGENTS[w.agent].color }} aria-hidden />
          {AGENTS[w.agent].label} · {windowLabel(w.window)}
        </span>
        {level && (
          <span className={s.level}>
            <level.icon aria-hidden style={{ color: level.color }} />
            {level.label}
          </span>
        )}
      </div>
      <div className={s.value}>
        {w.expired ? (
          <span className={s.muted}>Window reset</span>
        ) : (
          <>
            {Math.round(w.used_percent)}
            <span className={s.unit}>% used</span>
          </>
        )}
      </div>
      <div
        className={s.track}
        role="meter"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct}
        aria-label={`${windowLabel(w.window)} used`}
      >
        <div className={s.fill} style={{ width: `${pct}%`, background: level?.color ?? 'var(--heat-3)' }} />
      </div>
      <div className={s.meta}>
        {w.expired
          ? `Last reading ${Math.round(w.used_percent)}%, ${formatAgo(w.at)}`
          : resetMs != null && resetMs > 0
            ? `Resets in ${untilLabel(resetMs)} · ${resetMs > 86_400_000 ? formatDate(w.resets_at!) : formatTime(w.resets_at!)}`
            : `Read ${formatAgo(w.at)}`}
        {w.plan && <span>{w.plan.charAt(0).toUpperCase() + w.plan.slice(1)} plan</span>}
      </div>
      {points && points.length > 1 && (
        <UsageChart
          points={points}
          now={now}
          label={windowLabel(w.window)}
          breakMs={Math.min(w.window_minutes ?? 300, 1440) * 60_000}
        />
      )}
    </div>
  )
}
