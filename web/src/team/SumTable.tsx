import { useState, type ReactNode } from 'react'
import type { Column } from '../lib/csv'
import { formatCost, formatHours, formatNumber } from '../lib/format'
import { keptPct } from '../lib/lasted'
import { sortRows } from '../lib/sort'
import { CsvButton } from '../ui/CsvButton'
import { LastedBar } from '../ui/LastedBar'
import { Panel } from '../ui/Panel'
import { SortTh } from '../ui/SortTh'
import v from '../pages/Views.module.css'
import { followed, share, type Sum } from './api'
import s from './Team.module.css'

const SORT: Record<string, (x: Sum) => number> = {
  sessions: (x) => x.sessions,
  machines: (x) => x.machines,
  time: (x) => x.active_ms,
  cost: (x) => x.cost_usd,
  retries: (x) => (x.active_ms ? x.retry_ms / x.active_ms : -1),
  added: (x) => x.lasted.added,
  kept: (x) => keptPct(x.lasted) ?? -1,
  followed: (x) => followed(x) ?? -1,
  findings: (x) => x.findings,
}

const round = (n: number | null) => (n == null ? null : Math.round(n * 10) / 10)

function sumColumns<T extends Sum>(): Column<T>[] {
  return [
    { label: 'Sessions', value: (x) => x.sessions },
    { label: 'Machines', value: (x) => x.machines },
    { label: 'Agent hours', value: (x) => round(x.active_ms / 3_600_000) },
    { label: 'Est. cost (USD)', value: (x) => round(x.cost_usd) },
    { label: 'Share in retries (%)', value: (x) => (x.active_ms ? round((100 * x.retry_ms) / x.active_ms) : null) },
    { label: 'Lines added', value: (x) => x.lasted.added },
    { label: 'Still there (%)', value: (x) => round(keptPct(x.lasted)) },
    { label: 'Rules followed (%)', value: (x) => {
      const f = followed(x)
      return f == null ? null : round(100 * f)
    } },
    { label: 'Findings', value: (x) => x.findings },
  ]
}

/**
 * One Insights table: a first column naming the row, then the same sums for
 * every row. Columns sort; the CSV holds the rows in the order shown.
 */
export function SumTable<T extends Sum>({
  title,
  icon,
  csv,
  first,
  rows,
  rowKey,
  name,
  cell,
}: {
  title: string
  icon: ReactNode
  /** File name part for the CSV. */
  csv: string
  /** Header of the first column. */
  first: string
  rows: T[]
  rowKey: (x: T) => string
  /** The first column as text, for sorting and the CSV. */
  name: (x: T) => string
  cell: (x: T) => ReactNode
}) {
  // Empty: the order the server sent (weeks newest first, others largest first).
  const [sort, setSort] = useState('')
  const shown = sortRows(rows, sort, { ...SORT, name: (x: T) => name(x) })
  const columns: Column<T>[] = [{ label: first, value: name }, ...sumColumns<T>()]
  const th = (id: string, label: string, num = true) => <SortTh id={id} label={label} sort={sort} onSort={setSort} num={num} />
  return (
    <Panel title={title} icon={icon} flush actions={<CsvButton name={csv} rows={shown} columns={columns} />}>
      <div className={v.tableWrap}>
        <table className={`${v.table} ${s.tight}`}>
          <thead>
            <tr>
              {th('name', first, false)}
              {th('sessions', 'Sessions')}
              {th('machines', 'Machines')}
              {th('time', 'Agent time')}
              {th('cost', 'Est. cost')}
              {th('retries', 'In retries')}
              <SortTh id="added" label="Lines added" sort={sort} onSort={setSort} />
              {th('kept', 'Still there')}
              {th('followed', 'Rules followed')}
              {th('findings', 'Findings')}
            </tr>
          </thead>
          <tbody>
            {shown.map((x) => {
              const k = keptPct(x.lasted)
              return (
                <tr key={rowKey(x)}>
                  <td>{cell(x)}</td>
                  <td className={v.num}>{formatNumber(x.sessions)}</td>
                  <td className={v.num}>{formatNumber(x.machines)}</td>
                  <td className={v.num}>{formatHours(x.active_ms)}</td>
                  <td className={v.num}>{formatCost(x.cost_usd)}</td>
                  <td className={v.num}>{share(x.active_ms ? x.retry_ms / x.active_ms : null)}</td>
                  <td className={s.barCell}>
                    <LastedBar c={x.lasted} />
                  </td>
                  <td className={v.num}>{k == null ? '–' : `${Math.round(k)}%`}</td>
                  <td className={v.num}>{share(followed(x))}</td>
                  <td className={v.num}>{formatNumber(x.findings)}</td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </Panel>
  )
}
