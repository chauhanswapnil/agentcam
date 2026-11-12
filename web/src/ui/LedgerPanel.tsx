import { ListChecks } from 'lucide-react'
import type { Ledger, Measured } from '../lib/api'
import { formatCompact, formatCost, formatDate, formatHours } from '../lib/format'
import { Button } from './Button'
import { CodeText } from './Code'
import { Panel } from './Panel'
import s from './LedgerPanel.module.css'

const rate = (m: Measured, v: number | null) =>
  v == null ? '–' : m.kind === 'cache' ? formatCost(v) : m.kind === 'loud_command' ? `${formatCompact(v)} tokens` : formatHours(v)

/** What a measured fix came to, in plain words. Never a score. */
function outcome(m: Measured): string {
  if (!m.enough) {
    const days = Math.ceil(m.days_left)
    return days > 0 ? `Measuring: ${days} more ${days === 1 ? 'day' : 'days'}` : 'Measuring: a few more sessions needed'
  }
  if (m.change == null) return 'Nothing to compare with before'
  const pct = Math.round(Math.abs(m.change) * 100)
  if (pct < 5) return 'No change yet'
  return m.change < 0 ? `${pct}% less` : `${pct}% more`
}

/** Fixes marked done, with before and after side by side, and fixes set aside. */
export function LedgerPanel({ data, onClear }: { data: Ledger; onClear: (id: string) => void }) {
  if (data.applied.length === 0 && data.dismissed.length === 0) return null
  return (
    <Panel title="Ledger" icon={<ListChecks />}>
      {data.applied.length > 0 && (
        <ul className={s.list}>
          {data.applied.map((m) => (
            <li key={m.id}>
              <div className={s.main}>
                <span className={s.title}>
                  <CodeText text={m.title} />
                </span>
                <span className={s.sub}>Marked done {formatDate(m.at)}</span>
              </div>
              <div className={s.nums} title="Per 10 hours of agent time, so a busy week and a quiet one compare fairly. The four weeks before against the time since.">
                <span>
                  <i>Before</i>
                  <b>{rate(m, m.before_rate)}</b>
                </span>
                <span>
                  <i>Since</i>
                  <b>{m.after.sessions > 0 ? rate(m, m.after_rate) : '–'}</b>
                </span>
                <span className={s.outcome}>{outcome(m)}</span>
              </div>
              <Button size="sm" variant="ghost" onClick={() => onClear(m.id)} title="Put this fix back among the suggestions">
                Undo
              </Button>
            </li>
          ))}
        </ul>
      )}
      {data.applied.length > 0 && (
        <p className={s.note}>
          Before and since are per 10 hours of agent time. The change is stated only after a full week and five sessions, and it says so when a fix
          made no difference.
        </p>
      )}
      {data.dismissed.length > 0 && (
        <>
          <div className={s.head}>Set aside</div>
          <ul className={s.list}>
            {data.dismissed.map((m) => (
              <li key={m.id}>
                <div className={s.main}>
                  <span className={s.title}>
                    <CodeText text={m.title} />
                  </span>
                  <span className={s.sub}>Set aside {formatDate(m.at)}</span>
                </div>
                <Button size="sm" variant="ghost" onClick={() => onClear(m.id)}>
                  Bring back
                </Button>
              </li>
            ))}
          </ul>
        </>
      )}
    </Panel>
  )
}
