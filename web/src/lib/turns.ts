import { Ban, ShieldCheck, ShieldX, UserCheck, UserX, type LucideIcon } from 'lucide-react'

/** How a tool call was allowed or stopped, in plain words. Allowed calls
 * get a quiet icon; stopped ones a warning, always with the word. */
export const APPROVALS: Record<string, { label: string; short: string; icon: LucideIcon; color: string; stopped: boolean }> = {
  rule: { label: 'Allowed by a rule', short: 'Rule', icon: ShieldCheck, color: 'var(--text-3)', stopped: false },
  auto: { label: 'Allowed by auto mode', short: 'Auto mode', icon: ShieldCheck, color: 'var(--text-3)', stopped: false },
  mode: { label: 'Allowed by the permission mode', short: 'Mode', icon: ShieldCheck, color: 'var(--text-3)', stopped: false },
  config: { label: 'Allowed by settings', short: 'Settings', icon: ShieldCheck, color: 'var(--text-3)', stopped: false },
  you: { label: 'Approved by you', short: 'You', icon: UserCheck, color: 'var(--kind-you)', stopped: false },
  rejected: { label: 'Rejected by you', short: 'Rejected', icon: UserX, color: 'var(--status-warning)', stopped: true },
  'blocked-auto': { label: 'Blocked by auto mode', short: 'Auto mode blocked', icon: ShieldX, color: 'var(--status-warning)', stopped: true },
  'blocked-rule': { label: 'Blocked by a rule', short: 'Rule blocked', icon: Ban, color: 'var(--status-warning)', stopped: true },
  'blocked-safety': { label: 'Blocked by a safety check', short: 'Safety check', icon: ShieldX, color: 'var(--status-warning)', stopped: true },
}

export const approvalInfo = (a: string) =>
  APPROVALS[a] ?? { label: a, short: a, icon: ShieldCheck, color: 'var(--text-3)', stopped: false }

/** `skill:dataviz` → "dataviz skill"; `app:Gmail` → "Gmail app". */
export function viaLabel(via: string): string {
  const [kind, ...rest] = via.split(':')
  const name = rest.join(':')
  return kind === 'skill' ? `${name} skill` : kind === 'plugin' ? `${name} plugin` : kind === 'app' ? `${name} app` : via
}

/** Claude Code permission modes and Codex collaboration modes. */
export function modeLabel(m: string | null): string | null {
  if (!m) return null
  return (
    { default: 'Default', acceptEdits: 'Accept edits', plan: 'Plan', auto: 'Auto', bypassPermissions: 'Bypass permissions', dontAsk: "Don't ask" } as Record<
      string,
      string
    >
  )[m] ?? m
}
