import type { Lasted } from './api'

/** The three parts of a line count, in a fixed order with fixed colors.
 * One hue at three steps plus gray: none of them reads as good or bad. */
export const PARTS = [
  { key: 'kept', label: 'Still in the code', color: 'var(--heat-4)', hint: 'The file still has this line' },
  { key: 'reworked', label: 'Rewritten by agents', color: 'var(--heat-1)', hint: 'A later agent edit removed it' },
  { key: 'changed', label: 'Changed since', color: 'var(--line-3)', hint: 'Gone for another reason: edited by hand, another tool, a formatter, or the file moved' },
] as const

export type PartKey = (typeof PARTS)[number]['key']

/** Share of added lines still in the code, 0–100, or null when nothing was added. */
export const keptPct = (c: Lasted) => (c.added > 0 ? (100 * c.kept) / c.added : null)

export const pct = (n: number, of: number) => (of > 0 ? `${Math.round((100 * n) / of)}%` : '–')
