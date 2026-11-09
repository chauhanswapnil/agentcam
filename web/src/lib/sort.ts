/** Sort rows by `sort` (a key of `value`, `-` in front for ascending). */
export function sortRows<T>(rows: T[], sort: string, value: Record<string, (r: T) => number | string>): T[] {
  const asc = sort.startsWith('-')
  const get = value[asc ? sort.slice(1) : sort]
  if (!get) return rows
  return [...rows].sort((a, b) => {
    const x = get(a)
    const y = get(b)
    const c = typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y))
    return asc ? c : -c
  })
}

const SEVERITY_ORDER: Record<string, number> = { low: 1, medium: 2, high: 3, critical: 4 }

/** A severity as a number, so "critical" sorts above "high". */
export const severityNum = (s: string) => SEVERITY_ORDER[s] ?? 0
