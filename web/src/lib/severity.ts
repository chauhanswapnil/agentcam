import type { Severity } from './api'

/** Status colors, always shown with the shield icon and the word. */
export const SEVERITY_COLOR: Record<Severity, string> = {
  low: 'var(--text-3)',
  medium: 'var(--status-warning)',
  high: 'var(--status-serious)',
  critical: 'var(--status-critical)',
}

export const SEVERITY_RANK: Record<Severity, number> = { low: 1, medium: 2, high: 3, critical: 4 }
