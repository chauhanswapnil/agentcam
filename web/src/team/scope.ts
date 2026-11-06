import { useSearch } from 'wouter'
import { query } from '../lib/api'

/** Range and team from the address, shared by every team page. */
export function useScope() {
  const p = new URLSearchParams(useSearch())
  const range = p.get('range') ?? '30d'
  const team = p.get('team') ?? ''
  return { range, team, q: query({ range, team: team || undefined }) }
}

const weekFmt = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
export const weekLabel = (day: string) => {
  const [y, m, d] = day.split('-').map(Number)
  return weekFmt.format(new Date(y, m - 1, d))
}
