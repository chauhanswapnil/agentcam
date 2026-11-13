import { Timer } from 'lucide-react'
import { Link } from 'wouter'
import type { TimeBar as Bar } from '../lib/api'
import { formatCompact, formatHours, formatNumber } from '../lib/format'
import { GROUPS } from '../lib/kinds'
import { Panel } from './Panel'
import s from './TimeBar.module.css'

type Part = { key: keyof Bar; what: string; label: string; hint: string; color: string; soft?: boolean }

/** The parts of agent time, in reading order. Colors are the event groups. */
const PARTS: Part[] = [
  { key: 'model_ms', what: 'time:model', label: 'Model working', hint: 'Thinking, writing and choosing the next call', color: GROUPS.agent.color },
  { key: 'tools_ms', what: 'time:tools', label: 'Tools running', hint: 'Commands, edits, reads and other calls', color: GROUPS.change.color },
  { key: 'approval_ms', what: 'time:approval', label: 'Waiting on approval', hint: 'An estimate: how long calls sat until you allowed them', color: GROUPS.you.color },
  { key: 'you_ms', what: 'time:you', label: 'Ready for you', hint: 'The agent had replied and was waiting for your next prompt. Each wait counts up to 5 minutes.', color: GROUPS.you.color, soft: true },
  { key: 'redo_ms', what: 'time:redo', label: 'Redoing', hint: 'Retries, runs of steps that errored, and reverts', color: GROUPS.error.color },
]

const share = (v: number, total: number) => (total > 0 ? Math.round((v / total) * 100) : 0)

/**
 * The bar alone, for a tight spot: one session's time in the replay header.
 * With `onPick`, each part is a button that gets the key of its steps.
 */
export function TimeStrip({ data, onPick }: { data: Bar; onPick?: (what: string) => void }) {
  const total = data.total_ms
  const shown = PARTS.filter((p) => (data[p.key] as number) > 0)
  if (total <= 0) return null
  const label = shown.map((p) => `${p.label} ${formatHours(data[p.key] as number)} (${share(data[p.key] as number, total)}%)`).join(' · ')
  return (
    <div
      className={`${s.bar} ${s.strip}`}
      {...(onPick ? { role: 'group' } : { role: 'img' })}
      aria-label={`Where this session's time went: ${label}`}
      title={label}
    >
      {shown.map((p) =>
        onPick ? (
          <button
            key={p.key}
            type="button"
            className={`${s.seg} ${p.soft ? s.soft : ''}`}
            style={{ flexGrow: data[p.key] as number, background: p.color }}
            aria-label={`${p.label}, ${formatHours(data[p.key] as number)}: go to the next step like this`}
            title={`${p.label} ${formatHours(data[p.key] as number)}: go to the next step like this`}
            onClick={() => onPick(p.what)}
          />
        ) : (
          <span key={p.key} className={p.soft ? s.soft : undefined} style={{ flexGrow: data[p.key] as number, background: p.color }} />
        ),
      )}
    </div>
  )
}

/**
 * Where agent time went, as one split bar with a legend. With `onPick`, each
 * part opens the steps behind it; without it nothing here is clickable.
 */
export function TimeBar({
  data,
  title = 'Where the time went',
  onPick,
}: {
  data: Bar
  title?: string
  /** Called with a key for `/api/behind`, like `time:tools`. */
  onPick?: (what: string) => void
}) {
  // Parts of the start too small to matter aren't worth a sentence.
  const skills = (data.start_skills_tokens ?? 0) >= 1000 ? data.start_skills_tokens : null
  const mcp = (data.start_mcp_tokens ?? 0) >= 1000 ? data.start_mcp_tokens : null
  const total = data.total_ms
  const shown = PARTS.filter((p) => (data[p.key] as number) > 0)
  const label = shown.map((p) => `${p.label} ${share(data[p.key] as number, total)}%`).join(', ')
  return (
    <Panel title={title} icon={<Timer />}>
      <div className={s.bar} {...(onPick ? { role: 'group', 'aria-label': 'Where the time went' } : { role: 'img', 'aria-label': label })}>
        {shown.map((p) => {
          const v = data[p.key] as number
          const style = { flexGrow: v, background: p.color }
          return onPick ? (
            <button
              key={p.key}
              type="button"
              className={`${s.seg} ${p.soft ? s.soft : ''}`}
              style={style}
              aria-label={`${p.label}, ${formatHours(v)}, ${share(v, total)}%: show the steps`}
              title={`${p.label}: show the steps`}
              onClick={() => onPick(p.what)}
            />
          ) : (
            <span key={p.key} className={p.soft ? s.soft : undefined} style={style} />
          )
        })}
      </div>
      <ul className={s.legend}>
        {PARTS.map((p) => {
          const v = data[p.key] as number
          const inner = (
            <>
              <span className={`${s.dot} ${p.soft ? s.soft : ''}`} style={{ background: p.color }} aria-hidden />
              <span className={s.name}>
                {p.label}
                {p.key === 'approval_ms' && <i> est.</i>}
              </span>
              <b>{formatHours(v)}</b>
              <span className={s.share}>{share(v, total)}%</span>
            </>
          )
          return (
            <li key={p.key} title={p.hint} className={v === 0 ? s.none : undefined}>
              {onPick && v > 0 ? (
                <button type="button" className={`${s.item} ${s.pick}`} onClick={() => onPick(p.what)} aria-label={`${p.label}, ${formatHours(v)}, ${share(v, total)}%: show the steps`}>
                  {inner}
                </button>
              ) : (
                <span className={s.item}>{inner}</span>
              )}
            </li>
          )
        })}
      </ul>
      {(data.orienting_ms > 0 || data.approved_by_hand > 0) && (
        <p className={s.note}>
          {data.orienting_ms > 0 && (
            <>
              <b>{formatHours(data.orienting_ms)}</b> of the model and tool time went on reading and searching before the first change of a turn
              {onPick ? (
                <>
                  {' '}
                  (
                  <button type="button" className={s.textBtn} onClick={() => onPick('time:orienting')}>
                    show the turns
                  </button>
                  ).{' '}
                </>
              ) : (
                '. '
              )}
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
              {skills && (
                <>
                  {` ${formatCompact(skills)} is the `}
                  {onPick ? <Link href="/skills" className={s.textBtn}>skill listing</Link> : 'skill listing'}
                </>
              )}
              {skills && mcp ? ' and' : ''}
              {mcp && (
                <>
                  {` ${formatCompact(mcp)} is `}
                  {onPick ? <Link href="/setup" className={s.textBtn}>MCP server instructions</Link> : 'MCP server instructions'}
                </>
              )}
              .
            </>
          )}
        </p>
      )}
    </Panel>
  )
}
