import { ChevronRight, ListChecks } from 'lucide-react'
import { useState } from 'react'
import type { Ledger, LedgerWeek, Measured } from '../lib/api'
import { useApi } from '../lib/api'
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

/** The measure per 10 hours of agent time in one week, or null when the agent had no time that week. */
const weekRate = (w: LedgerWeek) => (w.agent_ms > 0 ? (w.value * 36_000_000) / w.agent_ms : null)

/**
 * A fix's weeks as bars, per 10 hours of agent time, with a line at the
 * week it was marked done.
 */
export function WeeksChart({ weeks, kind, markedAt }: { weeks: LedgerWeek[]; kind: string; markedAt: string }) {
  const rates = weeks.map(weekRate)
  const max = Math.max(...rates.map((r) => r ?? 0), 0.000001)
  // The week the mark falls in: the last one that began before it.
  const at = Date.parse(markedAt)
  const marked = weeks.reduce((n, w, i) => (Date.parse(w.start) <= at ? i : n), -1)
  const fake = { kind } as Measured
  return (
    <ol className={s.weeks} aria-label="By week, oldest first">
      {weeks.map((w, i) => {
        const r = rates[i]
        const label = `Week of ${formatDate(w.start)}: ${w.count} ${w.count === 1 ? 'time' : 'times'}, ${r == null ? 'no agent time' : `${rate(fake, r)} per 10 hours`}`
        return (
          <li key={w.start} className={i === marked ? s.markedWeek : undefined} title={label}>
            <span className={s.col}>
              <span className={s.weekBar} style={{ height: `${((r ?? 0) / max) * 100}%` }} />
            </span>
            <span className="sr-only">{label}</span>
            {i === marked && <span className={s.markedLabel}>marked done</span>}
          </li>
        )
      })}
    </ol>
  )
}

function Expanded({ m, onOpen }: { m: Measured; onOpen: (what: string, since?: string) => void }) {
  const weeks = useApi<LedgerWeek[]>(`/api/ledger/weeks?id=${encodeURIComponent(m.id)}`)
  return (
    <div className={s.more}>
      {weeks.data ? <WeeksChart weeks={weeks.data} kind={m.kind} markedAt={m.at} /> : <span className={s.sub}>Loading the weeks…</span>}
      <Button size="sm" variant="ghost" onClick={() => onOpen(`fix:${m.id}`, m.at)}>
        Runs since {formatDate(m.at)}
      </Button>
    </div>
  )
}

/**
 * Fixes marked done, with before and after side by side, and fixes set aside.
 * With `onOpen`, each row expands to its weeks and a list of the runs since.
 */
export function LedgerPanel({
  data,
  onClear,
  onOpen,
}: {
  data: Ledger
  onClear: (id: string) => void
  onOpen?: (what: string, since?: string) => void
}) {
  const [open, setOpen] = useState<string | null>(null)
  if (data.applied.length === 0 && data.dismissed.length === 0 && !data.marks_error) return null
  return (
    <Panel title="Ledger" icon={<ListChecks />}>
      {data.marks_error && (
        <p className={s.note}>
          Your <code>profile.json</code> couldn't be read, so fixes you marked aren't shown. Marking a fix starts a new file and keeps the old one
          as <code>profile.json.bad</code>. {data.marks_error}
        </p>
      )}
      {data.applied.length > 0 && (
        <ul className={s.list}>
          {data.applied.map((m) => (
            <li key={m.id} className={s.applied}>
              <div className={s.main}>
                {onOpen ? (
                  <button
                    type="button"
                    className={s.expand}
                    aria-expanded={open === m.id}
                    onClick={() => setOpen(open === m.id ? null : m.id)}
                  >
                    <ChevronRight aria-hidden className={open === m.id ? s.chevOpen : s.chev} />
                    <span className={s.title}>
                      <CodeText text={m.title} />
                    </span>
                  </button>
                ) : (
                  <span className={s.title}>
                    <CodeText text={m.title} />
                  </span>
                )}
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
              {onOpen && open === m.id && <Expanded m={m} onOpen={onOpen} />}
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
