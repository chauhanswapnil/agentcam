import { useCallback } from 'react'
import { useLocation, useSearch } from 'wouter'
import { formatCompact, formatCost, formatHours, formatNumber } from './format'

/**
 * The number whose steps are open in the drawer, kept in `?behind=` so the
 * drawer can be linked to and the Back button closes it.
 * Returns [open key or null, open(key), close()].
 */
export function useBehind(): [string | null, (what: string) => void, () => void] {
  const search = useSearch()
  const [path, navigate] = useLocation()
  const open = useCallback(
    (what: string) => {
      const q = new URLSearchParams(search)
      q.set('behind', what)
      // A new history entry, so Back goes to the page as it was.
      navigate(`${path}?${q.toString()}`)
    },
    [search, path, navigate],
  )
  const close = useCallback(() => {
    const q = new URLSearchParams(search)
    q.delete('behind')
    const rest = q.toString()
    navigate(rest ? `${path}?${rest}` : path)
  }, [search, path, navigate])
  return [new URLSearchParams(search).get('behind') || null, open, close]
}

/** How one row's share of the number reads, by what the number is in. */
export function formatBehind(value: number, unit: string): string {
  if (unit === 'ms') return formatHours(value)
  if (unit === 'usd') return formatCost(value)
  if (unit === 'tokens') return `${formatCompact(value)} tokens`
  return formatNumber(value)
}
