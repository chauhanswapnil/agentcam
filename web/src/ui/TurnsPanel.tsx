import { MessagesSquare } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'wouter'
import type { TurnGroup, TurnStats } from '../lib/api'
import { formatCost, formatDuration, formatNumber, oneLine } from '../lib/format'
import { modeLabel } from '../lib/turns'
import { AgentLabel } from './Misc'
import { Panel } from './Panel'
import { Segmented } from './Segmented'
import { Tooltip } from './Tooltip'
import s from './TurnsPanel.module.css'

const LENGTHS = ['Under 30 s', '30 s – 2 min', '2 – 10 min', '10 – 30 min', 'Over 30 min']
const short = (ms: number) => formatDuration(ms).replace(/ \d+s$/, '')

/** How long turns take: a prompt you typed and the work until the next. */
export function TurnsPanel({ data }: { data: TurnStats }) {
  const [by, setBy] = useState<'model' | 'effort' | 'mode'>('model')
  const groups: TurnGroup[] = by === 'model' ? data.by_model : by === 'effort' ? data.by_effort : data.by_mode
  const maxLen = Math.max(1, ...data.lengths)
  const name = (g: TurnGroup) => (g.key == null ? 'Not recorded' : by === 'mode' ? (modeLabel(g.key) ?? g.key) : g.key)
  return (
    <Panel title="Turns" icon={<MessagesSquare />}>
      <div className={s.top}>
        <div className={s.stat}>
          <span>Turns</span>
          <b>{formatNumber(data.turns)}</b>
        </div>
        <div className={s.stat}>
          <span>Typical turn</span>
          <b>{short(data.median_ms)}</b>
        </div>
        <div className={s.stat}>
          <span>9 in 10 under</span>
          <b>{short(data.p90_ms)}</b>
        </div>
        <div className={s.stat}>
          <span>Steps per turn</span>
          <b>{data.median_steps}</b>
        </div>
        {data.median_wait_ms != null && (
          <div className={s.stat} title="Typical time from a turn ending to your next prompt, leaving out overnight gaps">
            <span>You reply in</span>
            <b>{short(data.median_wait_ms)}</b>
          </div>
        )}
      </div>

      <div className={s.lengths} role="img" aria-label={LENGTHS.map((l, i) => `${l}: ${data.lengths[i]}`).join(', ')}>
        {data.lengths.map((n, i) => (
          <Tooltip key={i} delay={80} content={`${LENGTHS[i]}: ${formatNumber(n)} turns`}>
            <span className={s.col}>
              <span className={s.colBar} style={{ height: `${Math.max(2, (n / maxLen) * 100)}%` }} />
              <span className={s.colLabel}>{LENGTHS[i]}</span>
            </span>
          </Tooltip>
        ))}
      </div>

      <div className={s.groupHead}>
        <span>By</span>
        <Segmented
          label="Group turns by"
          value={by}
          onChange={setBy}
          options={[
            { value: 'model', label: 'Model' },
            { value: 'effort', label: 'Effort' },
            { value: 'mode', label: 'Mode' },
          ]}
        />
      </div>
      <table className={s.table}>
        <thead>
          <tr>
            <th>{by === 'model' ? 'Model' : by === 'effort' ? 'Effort' : 'Mode'}</th>
            <th className={s.num}>Turns</th>
            <th className={s.num}>Typical length</th>
            <th className={s.num}>Steps</th>
            <th className={s.num}>Avg cost</th>
          </tr>
        </thead>
        <tbody>
          {groups.slice(0, 8).map((g) => (
            <tr key={g.key ?? '-'}>
              <td className={g.key == null ? s.muted : undefined}>
                {g.key == null ? (
                  name(g)
                ) : (
                  <Link href={`/compare?dim=${by}&a=${encodeURIComponent(g.key)}`} className={s.cmp} title="Compare turns with this value against another">
                    {name(g)}
                  </Link>
                )}
              </td>
              <td className={s.num}>{formatNumber(g.turns)}</td>
              <td className={s.num}>{g.key == null && g.median_ms === 0 ? '–' : short(g.median_ms)}</td>
              <td className={s.num}>{g.median_steps}</td>
              <td className={s.num}>{g.avg_cost_usd != null ? formatCost(g.avg_cost_usd) : '–'}</td>
            </tr>
          ))}
        </tbody>
      </table>

      {data.longest.length > 0 && (
        <>
          <div className={s.groupHead}>
            <span>Longest turns</span>
          </div>
          <ol className={s.longest}>
            {data.longest.slice(0, 5).map((x) => (
              <li key={`${x.session_id}-${x.turn.n}`}>
                <Link href={`/sessions/${x.session_id}?step=${x.turn.seq}`} className={s.long}>
                  <AgentLabel agent={x.agent} hideName />
                  <span className={s.longText}>{oneLine(x.turn.prompt || x.title || 'Prompt')}</span>
                  <span className={s.longMeta}>
                    {x.turn.steps} steps · {x.turn.duration_ms != null ? short(x.turn.duration_ms) : '–'}
                  </span>
                </Link>
              </li>
            ))}
          </ol>
        </>
      )}
    </Panel>
  )
}
