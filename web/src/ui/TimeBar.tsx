import { Timer } from 'lucide-react'
import type { TimeBar as Bar } from '../lib/api'
import { formatCompact, formatHours, formatNumber } from '../lib/format'
import { GROUPS } from '../lib/kinds'
import { Panel } from './Panel'
import s from './TimeBar.module.css'

type Part = { key: keyof Bar; label: string; hint: string; color: string; soft?: boolean }

/** The parts of agent time, in reading order. Colors are the event groups. */
const PARTS: Part[] = [
  { key: 'model_ms', label: 'Model working', hint: 'Thinking, writing and choosing the next call', color: GROUPS.agent.color },
  { key: 'tools_ms', label: 'Tools running', hint: 'Commands, edits, reads and other calls', color: GROUPS.change.color },
  { key: 'approval_ms', label: 'Waiting on approval', hint: 'An estimate: how long calls sat until you allowed them', color: GROUPS.you.color },
  { key: 'you_ms', label: 'Ready for you', hint: 'The agent had replied and was waiting for your next prompt. Each wait counts up to 5 minutes.', color: GROUPS.you.color, soft: true },
  { key: 'redo_ms', label: 'Redoing', hint: 'Retries, runs of steps that errored, and reverts', color: GROUPS.error.color },
]

const share = (v: number, total: number) => (total > 0 ? Math.round((v / total) * 100) : 0)

/** The bar alone, for a tight spot: one session's time in the replay header. */
export function TimeStrip({ data }: { data: Bar }) {
  const total = data.total_ms
  const shown = PARTS.filter((p) => (data[p.key] as number) > 0)
  if (total <= 0) return null
  const label = shown.map((p) => `${p.label} ${formatHours(data[p.key] as number)} (${share(data[p.key] as number, total)}%)`).join(' · ')
  return (
    <div className={`${s.bar} ${s.strip}`} role="img" aria-label={`Where this session's time went: ${label}`} title={label}>
      {shown.map((p) => (
        <span key={p.key} className={p.soft ? s.soft : undefined} style={{ flexGrow: data[p.key] as number, background: p.color }} />
      ))}
    </div>
  )
}

/** Where agent time went, as one split bar with a legend. */
export function TimeBar({ data, title = 'Where the time went' }: { data: Bar; title?: string }) {
  // Parts of the start too small to matter aren't worth a sentence.
  const skills = (data.start_skills_tokens ?? 0) >= 1000 ? data.start_skills_tokens : null
  const mcp = (data.start_mcp_tokens ?? 0) >= 1000 ? data.start_mcp_tokens : null
  const total = data.total_ms
  const shown = PARTS.filter((p) => (data[p.key] as number) > 0)
  const label = shown.map((p) => `${p.label} ${share(data[p.key] as number, total)}%`).join(', ')
  return (
    <Panel title={title} icon={<Timer />}>
      <div className={s.bar} role="img" aria-label={label}>
        {shown.map((p) => (
          <span
            key={p.key}
            className={p.soft ? s.soft : undefined}
            style={{ flexGrow: data[p.key] as number, background: p.color }}
          />
        ))}
      </div>
      <ul className={s.legend}>
        {PARTS.map((p) => {
          const v = data[p.key] as number
          return (
            <li key={p.key} title={p.hint} className={v === 0 ? s.none : undefined}>
              <span className={`${s.dot} ${p.soft ? s.soft : ''}`} style={{ background: p.color }} aria-hidden />
              <span className={s.name}>
                {p.label}
                {p.key === 'approval_ms' && <i> est.</i>}
              </span>
              <b>{formatHours(v)}</b>
              <span className={s.share}>{share(v, total)}%</span>
            </li>
          )
        })}
      </ul>
      {(data.orienting_ms > 0 || data.approved_by_hand > 0) && (
        <p className={s.note}>
          {data.orienting_ms > 0 && (
            <>
              <b>{formatHours(data.orienting_ms)}</b> of the model and tool time went on reading and searching before the first change of a turn.{' '}
            </>
          )}
          {data.approved_by_hand > 0 && (
            <>
              You allowed or turned down {formatNumber(data.approved_by_hand)} {data.approved_by_hand === 1 ? 'call' : 'calls'} by hand.
            </>
          )}
        </p>
      )}
      {data.start_tokens != null && data.start_tokens > 0 && (
        <p className={s.note}>
          A session starts at about <b>{formatCompact(data.start_tokens)} tokens</b> before any work: the agent's own prompt, tool definitions, your
          instruction files and your first message.
          {(skills || mcp) && (
            <>
              {' '}
              Of that, an estimated
              {skills ? ` ${formatCompact(skills)} is the skill listing` : ''}
              {skills && mcp ? ' and' : ''}
              {mcp ? ` ${formatCompact(mcp)} is MCP server instructions` : ''}.
            </>
          )}
        </p>
      )}
    </Panel>
  )
}
