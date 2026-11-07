import { Circle, CircleCheck, CircleDot, ListChecks, NotebookPen } from 'lucide-react'
import { useState } from 'react'
import type { PlanTask, SessionPlan } from '../lib/api'
import { Markdown } from './Markdown'
import { Panel } from './Panel'
import s from './PlanPanel.module.css'

const ICON = { completed: CircleCheck, in_progress: CircleDot, pending: Circle }
const WORD = { completed: 'Done', in_progress: 'In progress', pending: 'To do' }

/**
 * The agent's own task list as it stood at the playhead, and the plans it
 * proposed. From TodoWrite / TaskCreate / TaskUpdate, Codex update_plan
 * and ExitPlanMode (`plans.rs`).
 */
export function PlanPanel({ plan, step, onSeek }: { plan: SessionPlan; step: number; onSeek: (i: number) => void }) {
  const [open, setOpen] = useState<number | null>(null)
  const snaps = plan.snapshots
  const at = [...snaps].reverse().find((x) => x.seq <= step)
  const shown = at ?? snaps[0]
  const done = shown ? shown.tasks.filter((t) => t.status === 'completed').length : 0
  return (
    <Panel
      title={shown ? `Plan · ${done} of ${shown.tasks.length} done` : 'Plan'}
      icon={<ListChecks />}
      actions={
        shown && (
          <button type="button" className={s.at} onClick={() => onSeek(shown.seq)}>
            {at ? `as of step ${shown.seq + 1}` : `starts at step ${shown.seq + 1}`}
          </button>
        )
      }
      flush
    >
      {shown && (
        <ul className={`${s.tasks} ${at ? '' : s.later}`}>
          {shown.tasks.map((t, i) => (
            <TaskRow key={i} t={t} />
          ))}
        </ul>
      )}
      {snaps.length > 1 && (
        <div className={s.changes}>
          {snaps.map((x, i) => (
            <button
              key={x.seq}
              type="button"
              className={`${s.tick} ${x === at ? s.tickOn : ''}`}
              onClick={() => onSeek(x.seq)}
              title={`Update ${i + 1} at step ${x.seq + 1}`}
              aria-label={`Plan update ${i + 1}, step ${x.seq + 1}`}
            />
          ))}
        </div>
      )}
      {plan.plans.map((p) => (
        <div key={p.seq} className={s.doc}>
          <button type="button" className={s.docHead} onClick={() => setOpen(open === p.seq ? null : p.seq)} aria-expanded={open === p.seq}>
            <NotebookPen aria-hidden />
            Plan proposed at step {p.seq + 1}
          </button>
          {open === p.seq && <Markdown className={s.docBody} text={p.text} />}
        </div>
      ))}
    </Panel>
  )
}

function TaskRow({ t }: { t: PlanTask }) {
  const Icon = ICON[t.status]
  return (
    <li className={`${s.task} ${s[t.status]}`}>
      <Icon aria-hidden />
      <span>{t.text}</span>
      <span className="sr-only">{WORD[t.status]}</span>
    </li>
  )
}
