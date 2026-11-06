import { Activity, CircleDot, Hourglass, type LucideIcon } from 'lucide-react'
import type { LiveState } from './api'

/** Where a session was started from, in plain words. */
export function entrypointLabel(e: string | null | undefined): string | null {
  if (!e) return null
  const k = e.toLowerCase()
  if (k === 'cli' || k.startsWith('codex_cli') || k === 'codex_exec') return 'Terminal'
  if (k.includes('desktop')) return 'Desktop app'
  if (k.includes('vscode')) return 'VS Code'
  if (k.includes('jetbrains')) return 'JetBrains'
  if (k.startsWith('sdk')) return 'Agent SDK'
  if (k.includes('web')) return 'Web'
  return e
}

/** The state dot: color on the mark, the word beside it. */
export const STATES: Record<LiveState, { label: string; color: string; icon: LucideIcon; hint: string }> = {
  working: {
    label: 'Working',
    color: 'var(--status-good)',
    icon: Activity,
    hint: 'A step in the last 2 minutes, or a command still running',
  },
  waiting: {
    label: 'Your turn',
    color: 'var(--kind-you)',
    icon: Hourglass,
    hint: 'The agent replied; it may be waiting for you',
  },
  quiet: { label: 'Quiet', color: 'var(--text-3)', icon: CircleDot, hint: 'No step for a few minutes' },
}

/** `5h` → "5-hour window", `weekly` → "Weekly", `session` → "Session limit". */
export function windowLabel(w: string): string {
  if (w === '5h') return '5-hour window'
  if (w === 'weekly') return 'Weekly'
  const m = /^(\d+)([mhd])$/.exec(w)
  if (m) return `${m[1]}${m[2]} window`
  const words = w.replace(/-/g, ' ')
  return words.charAt(0).toUpperCase() + words.slice(1) + ' limit'
}

/** Level for a usage meter: under 75% is fine, then near, then reached. */
export function usageLevel(pct: number): 'ok' | 'near' | 'reached' {
  if (pct >= 100) return 'reached'
  if (pct >= 75) return 'near'
  return 'ok'
}
