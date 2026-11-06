import { FlaskConical } from 'lucide-react'
import type { ClaudeQuota } from '../lib/api'
import { AGENTS } from '../lib/agents'
import { formatAgo, formatCompact, formatCost, formatNumber, formatTime } from '../lib/format'
import { untilLabel } from '../lib/live'
import { Badge } from './Badge'
import { UsageChart } from './UsageChart'
import m from './UsageMeter.module.css'
import s from './QuotaMeter.module.css'

const ESTIMATE_HINT =
  "Estimated from the token counts in this machine's Claude Code logs, in 5-hour blocks the way ccusage counts them. Claude Code doesn't log its real limits, so the reference is your highest past block, not a limit."

function Head({ label }: { label: string }) {
  return (
    <div className={m.top}>
      <span className={m.name}>
        <span className={m.dot} style={{ background: AGENTS['claude-code'].color }} aria-hidden />
        Claude Code · {label}
      </span>
      <Badge icon={<FlaskConical size={12} aria-hidden />} title={ESTIMATE_HINT}>
        Estimate
      </Badge>
    </div>
  )
}

/** Claude Code's 5-hour block and last 7 days, estimated from logged tokens. */
export function QuotaMeter({ q, now }: { q: ClaudeQuota; now: number }) {
  const a = q.active
  const peak = q.peak?.total_tokens ?? 0
  const pct = a && peak > 0 ? (100 * a.total_tokens) / peak : null
  const projPct = q.pace && peak > 0 ? (100 * q.pace.projected_tokens) / peak : null
  const endMs = a ? new Date(a.end).getTime() - now : 0
  // Each block is a flat run from its first call to its latest one.
  const points =
    peak > 0
      ? q.blocks.flatMap((b) => {
          const v = (100 * b.total_tokens) / peak
          return [
            { ts: b.first_at, used_percent: v },
            { ts: b.last_at, used_percent: v },
          ]
        })
      : []
  const unpriced = q.week.unpriced_calls > 0 ? '+' : ''
  return (
    <>
      <div className={m.card}>
        <Head label="5-hour block" />
        <div className={m.value}>
          {a ? (
            <>
              {formatCompact(a.total_tokens)}
              <span className={m.unit}> tokens</span>
            </>
          ) : (
            <span className={m.muted}>No open block</span>
          )}
        </div>
        {pct != null && (
          <div
            className={m.track}
            style={{ position: 'relative' }}
            role="meter"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.min(100, Math.round(pct))}
            aria-label="Open block compared with your highest block"
          >
            <div className={m.fill} style={{ width: `${Math.min(100, pct)}%`, background: 'var(--heat-3)' }} />
            {projPct != null && projPct > pct && (
              <div className={s.projected} style={{ left: `${Math.min(100, pct)}%`, width: `${Math.max(0, Math.min(100, projPct) - Math.min(100, pct))}%` }} />
            )}
          </div>
        )}
        <div className={m.meta}>
          {a ? (
            <>
              <span>
                Ends in {untilLabel(Math.max(endMs, 60_000))} · {formatTime(a.end)}
              </span>
              <span>
                {formatCost(a.cost_usd)}
                {a.unpriced_calls ? '+' : ''} at API prices
              </span>
            </>
          ) : (
            <span>{q.blocks.length ? `Last block ended ${formatAgo(q.blocks[q.blocks.length - 1].end)}` : 'No block in the last 7 days'}</span>
          )}
        </div>
        {a && (
          <p className={s.line}>
            {pct != null && (
              <>
                <b>{Math.round(pct)}%</b> of your highest block ({formatCompact(peak)}).{' '}
              </>
            )}
            {q.pace ? (
              <>
                Pace {formatCompact(q.pace.tokens_per_minute)}/min, {formatCost(q.pace.cost_per_hour)}/h; heading for{' '}
                <b>{formatCompact(q.pace.projected_tokens)}</b>
                {projPct != null && ` (${projPct > 200 ? 'over twice your highest' : `${Math.round(projPct)}%${projPct > 100 ? ', above your highest' : ''}`})`}{' '}
                by the end.
              </>
            ) : (
              'Too early for a pace.'
            )}
          </p>
        )}
        {points.length > 1 && <UsageChart points={points} now={now} label="Blocks compared with your highest" breakMs={q.block_minutes * 60_000} />}
      </div>
      <div className={m.card}>
        <Head label="Last 7 days" />
        <div className={m.value}>
          {formatCompact(q.week.total_tokens)}
          <span className={m.unit}> tokens</span>
        </div>
        <div className={m.meta}>
          <span>
            {formatCost(q.week.cost_usd)}
            {unpriced} at API prices
          </span>
          <span>{formatNumber(q.week.calls)} model calls</span>
        </div>
        <p className={s.line}>
          {formatCompact(q.week.output_tokens)} output, {formatCompact(q.week.input_tokens)} input,{' '}
          {formatCompact(q.week.cache_read_tokens)} cache reads, {formatCompact(q.week.cache_write_tokens)} cache writes. Counted
          from the logs; your plan's real weekly limit isn't logged.
        </p>
      </div>
    </>
  )
}
