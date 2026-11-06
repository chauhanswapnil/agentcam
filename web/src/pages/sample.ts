import type { ClaudeQuota, LimitWindow, QuotaBlock } from '../lib/api'
import type { StripEvent } from '../ui/EventStrip'

/** Made-up session for the styleguide. Not real data. */
export const SAMPLE_EVENTS: StripEvent[] = [
  { kind: 'prompt', title: 'The login form rejects valid passwords. Please fix it.' },
  { kind: 'thinking', title: 'Look at the auth module first.', durationMs: 1800 },
  { kind: 'response', title: 'Let me look at the auth code.' },
  { kind: 'tool_call', category: 'search', title: 'password', durationMs: 420 },
  { kind: 'tool_call', category: 'read', title: 'src/auth/check.ts', durationMs: 90 },
  { kind: 'tool_call', category: 'read', title: 'src/auth/session.ts', durationMs: 85 },
  { kind: 'thinking', title: 'The input is trimmed before comparing.', durationMs: 2600 },
  { kind: 'tool_call', category: 'edit', title: 'src/auth/check.ts', durationMs: 300 },
  { kind: 'tool_call', category: 'command', title: 'npm test', durationMs: 14200, isError: true },
  { kind: 'response', title: 'One test still expects trimming. Updating it.' },
  { kind: 'tool_call', category: 'edit', title: 'test/auth.test.ts', durationMs: 280 },
  { kind: 'tool_call', category: 'command', title: 'npm test', durationMs: 13800 },
  { kind: 'tool_call', category: 'web', title: 'OWASP password storage cheat sheet', durationMs: 2100 },
  { kind: 'tool_call', category: 'mcp', title: 'docs.lookup bcrypt cost factor', durationMs: 640 },
  { kind: 'tool_call', category: 'write', title: 'docs/auth.md', durationMs: 150 },
  { kind: 'tool_call', category: 'command', title: 'git diff --stat', durationMs: 120 },
  { kind: 'response', title: 'Fixed: passwords are no longer trimmed. Tests pass.' },
  { kind: 'prompt', title: 'Commit it' },
  { kind: 'tool_call', category: 'command', title: 'git commit -m "Stop trimming passwords"', durationMs: 900 },
  { kind: 'response', title: 'Committed as 4f2a91c.' },
]

export const SAMPLE_SESSIONS = [
  { agent: 'claude-code' as const, title: 'Fix the login bug', project: 'demo-app', branch: 'main', when: '2h ago', steps: 20, files: 3, errors: 1, duration: 312_000 },
  { agent: 'codex' as const, title: 'Add rate limiting to the login route', project: 'api', branch: 'feature/rate-limit', when: 'Yesterday', steps: 46, files: 2, errors: 0, duration: 1_284_000 },
  { agent: 'claude-code' as const, title: 'Migrate the settings screen to the new design', project: 'mobile', branch: 'design/v2', when: 'Nov 2', steps: 318, files: 27, errors: 4, duration: 7_380_000 },
]

export const SAMPLE_NOW = Date.parse('2026-11-06T12:00:00Z')

const base = { agent: 'codex' as const, plan: 'plus', session_id: 'x', at: '2026-11-06T11:58:00Z', expired: false }
export const SAMPLE_WINDOWS: LimitWindow[] = [
  { ...base, window: '5h', window_minutes: 300, used_percent: 42, resets_at: '2026-11-06T14:20:00Z' },
  { ...base, window: '5h', window_minutes: 300, used_percent: 86, resets_at: '2026-11-06T12:40:00Z' },
  { ...base, window: 'weekly', window_minutes: 10080, used_percent: 100, resets_at: '2026-11-09T08:00:00Z' },
]

export const SAMPLE_USAGE = Array.from({ length: 40 }, (_, i) => ({
  ts: new Date(SAMPLE_NOW - (40 - i) * 4 * 3_600_000).toISOString(),
  used_percent: Math.round(((i * 37) % 100) * 0.9),
}))

const block = (h: number, tokens: number, active = false): QuotaBlock => ({
  start: new Date(SAMPLE_NOW - h * 3_600_000).toISOString(),
  end: new Date(SAMPLE_NOW - (h - 5) * 3_600_000).toISOString(),
  first_at: new Date(SAMPLE_NOW - h * 3_600_000 + 600_000).toISOString(),
  last_at: new Date(SAMPLE_NOW - (active ? 0 : h - 3) * 3_600_000).toISOString(),
  active,
  calls: Math.round(tokens / 40_000),
  input_tokens: tokens * 0.001,
  output_tokens: tokens * 0.01,
  cache_read_tokens: tokens * 0.95,
  cache_write_tokens: tokens * 0.039,
  total_tokens: tokens,
  cost_usd: tokens / 900_000,
  unpriced_calls: 0,
})
const ACTIVE = block(2, 3_100_000, true)
export const SAMPLE_QUOTA: ClaudeQuota = {
  estimate: true,
  block_minutes: 300,
  active: ACTIVE,
  pace: { tokens_per_minute: 28_000, cost_per_hour: 1.9, projected_tokens: 8_100_000, projected_cost_usd: 9, remaining_ms: 3 * 3_600_000 },
  peak: block(50, 9_400_000),
  blocks: [block(120, 4_200_000), block(74, 6_000_000), block(50, 9_400_000), block(26, 2_800_000), ACTIVE],
  week: { ...block(0, 25_500_000), calls: 640 },
}
