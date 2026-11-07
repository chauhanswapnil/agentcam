import { Bot } from 'lucide-react'
import type { SessionRow } from '../lib/api'
import { formatCost, formatHours, formatNumber, oneLine } from '../lib/format'
import { Panel } from './Panel'
import s from './SubagentTree.module.css'

const cost = (x: SessionRow) => x.est_cost_usd ?? x.cost_usd ?? 0

/**
 * The session and the subagents it started, with each branch's share of
 * the cost and time. Click a subagent to open its own replay.
 */
export function SubagentTree({ parent, subagents, onOpen }: { parent: SessionRow; subagents: SessionRow[]; onOpen: (id: string) => void }) {
  const total = cost(parent) + subagents.reduce((a, x) => a + cost(x), 0)
  const time = parent.active_ms + subagents.reduce((a, x) => a + x.active_ms, 0)
  const rows = [...subagents].sort((a, b) => cost(b) - cost(a))
  const share = (x: SessionRow) => (total > 0 ? cost(x) / total : time > 0 ? x.active_ms / time : 0)
  return (
    <Panel title={`Subagents · ${subagents.length}`} icon={<Bot />} flush>
      <div className={s.head}>
        <span>{formatCost(total)} in all</span>
        <span>{formatHours(time)} agent time</span>
      </div>
      <ul className={s.tree}>
        <li className={s.row}>
          <span className={s.name}>This session</span>
          <Bar v={share(parent)} />
          <span className={s.num}>{formatCost(cost(parent))}</span>
        </li>
        {rows.map((x) => (
          <li key={x.id}>
            <button type="button" className={`${s.row} ${s.child}`} onClick={() => onOpen(x.id)}>
              <span className={s.name} title={x.title ?? x.first_prompt ?? x.id}>
                {oneLine(x.title ?? x.first_prompt ?? x.id)}
                <span className={s.meta}>
                  {formatNumber(x.event_count)} steps · {formatHours(x.active_ms)}
                  {x.error_count > 0 ? ` · ${x.error_count} errored` : ''}
                </span>
              </span>
              <Bar v={share(x)} />
              <span className={s.num}>{formatCost(cost(x))}</span>
            </button>
          </li>
        ))}
      </ul>
    </Panel>
  )
}

function Bar({ v }: { v: number }) {
  return (
    <span className={s.bar} aria-hidden>
      <span style={{ width: `${Math.max(2, Math.round(v * 100))}%` }} />
    </span>
  )
}
