import type { CacheCause, Rebuild } from './api'
import { formatCompact, formatDuration } from './format'

/** Each cause in plain words, and what the person can do about it. */
export const CAUSES: Record<CacheCause, { label: string; next: string }> = {
  break: {
    label: 'After a break',
    next: 'The cache had run out by the time you came back. At the end of a task, a fresh session costs less than returning to a big one; compacting before a break helps too.',
  },
  model_switch: {
    label: 'Model switched',
    next: 'Each model keeps its own cache. Pick the model at the start, or hand the other model’s work to a subagent.',
  },
  effort_change: {
    label: 'Effort changed',
    next: 'Effort is part of what the cache is keyed on. Set it once per session where you can.',
  },
  unknown: {
    label: 'Cause not in the log',
    next: 'Nothing the log records explains these. Tools or instructions changing mid-session can do it.',
  },
}

const away = (ms: number) => formatDuration(ms).replace(/ \d+s$/, '')

/** "212K tokens · after 34m away" or "… · claude-a → claude-b". */
export function describeRebuild(b: Rebuild): string {
  const parts = [`${formatCompact(b.tokens)} tokens`]
  if (b.cause === 'break' && b.gap_ms != null) parts.push(`after ${away(b.gap_ms)} away`)
  else if (b.cause === 'model_switch') parts.push(`${b.from_model ?? 'another model'} → ${b.model ?? 'another model'}`)
  else if (b.cause === 'effort_change') parts.push(`effort ${b.from_effort} → ${b.effort}`)
  else parts.push('cause not in the log')
  return parts.join(' · ')
}
