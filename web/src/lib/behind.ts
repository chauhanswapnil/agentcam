import { useCallback } from 'react'
import { useLocation, useSearch } from 'wouter'
import { formatCompact, formatCost, formatHours, formatNumber } from './format'

/**
 * The number whose steps are open in the drawer, kept in `?behind=` so the
 * drawer can be linked to and the Back button closes it.
 * `open` may also take a `since` time, kept in `?since=`, to list only what
 * came after it. Returns [open key or null, open(key, since?), close()].
 */
export function useBehind(): [string | null, (what: string, since?: string, until?: string) => void, () => void] {
  const search = useSearch()
  const [path, navigate] = useLocation()
  const open = useCallback(
    (what: string, since?: string, until?: string) => {
      const q = new URLSearchParams(search)
      q.set('behind', what)
      if (since) q.set('since', since)
      else q.delete('since')
      if (until) q.set('until', until)
      else q.delete('until')
      // A new history entry, so Back goes to the page as it was.
      navigate(`${path}?${q.toString()}`)
    },
    [search, path, navigate],
  )
  const close = useCallback(() => {
    const q = new URLSearchParams(search)
    q.delete('behind')
    q.delete('since')
    q.delete('until')
    const rest = q.toString()
    navigate(rest ? `${path}?${rest}` : path)
  }, [search, path, navigate])
  return [new URLSearchParams(search).get('behind') || null, open, close]
}

/** How one row's share of the number reads, by what the number is in. */
export function formatBehind(value: number, unit: string): string {
  if (unit === 'ms') return formatHours(value)
  if (unit === 'usd') return formatCost(value)
  if (unit === 'percent') return `${Math.round(value)}%`
  if (unit === 'none') return ''
  if (unit === 'tokens') return `${formatCompact(value)} tokens`
  return formatNumber(value)
}
