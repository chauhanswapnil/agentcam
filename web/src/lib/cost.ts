import type { Costs } from './api'
import { formatCost, formatNumber } from './format'

/** The four kinds of token a call is charged for, costliest first as a rule. */
export const COST_PARTS: { key: keyof Costs['parts']; label: string; hint: string }[] = [
  { key: 'output', label: 'Output', hint: 'What the model wrote, thinking included' },
  { key: 'cache_write', label: 'Cache writes', hint: 'Prompt stored for reuse; costs more than fresh input, once' },
  { key: 'cache_read', label: 'Cache reads', hint: 'Prompt reused from the cache, at a fraction of the input price' },
  { key: 'input', label: 'Fresh input', hint: 'Prompt tokens sent at the full input price' },
]

/** One line for a tooltip: where a session's estimated cost comes from. */
export function costSummary(c: Costs): string {
  const parts = COST_PARTS.filter((p) => c.parts[p.key] > 0)
    .map((p) => `${p.label} ${formatCost(c.parts[p.key])}`)
    .join(' · ')
  const left = c.unpriced_calls ? ` ${formatNumber(c.unpriced_calls)} calls have no price and are left out.` : ''
  return `${parts || 'No priced calls'}. At API list prices.${left}`
}
