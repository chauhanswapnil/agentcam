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

export const SAMPLE_COSTS: import('../lib/api').Costs = {
  usd: 41.9,
  parts: { input: 1.2, output: 14.6, cache_read: 7.3, cache_write: 18.8 },
  without_cache_usd: 163.4,
  cache_hit_rate: 0.93,
  calls: 2140,
  unpriced_calls: 25,
  unpriced_models: ['some-new-model'],
  fast_calls: 12,
  models: [
    { model: 'claude-opus-5-5', calls: 1480, input_tokens: 210_000, output_tokens: 540_000, cache_read_tokens: 31_000_000, cache_write_tokens: 1_900_000, usd: 33.1 },
    { model: 'gpt-6.1-sol', calls: 635, input_tokens: 900_000, output_tokens: 410_000, cache_read_tokens: 9_400_000, cache_write_tokens: 0, usd: 8.8 },
    { model: 'some-new-model', calls: 25, input_tokens: 40_000, output_tokens: 9_000, cache_read_tokens: 0, cache_write_tokens: 0, usd: null },
  ],
  priced_on: '2026-11-11',
  prices_error: null,
}

export const SAMPLE_TIME: import('../lib/api').TimeBar = {
  total_ms: 26 * 3_600_000,
  model_ms: 11.2 * 3_600_000,
  tools_ms: 4.1 * 3_600_000,
  approval_ms: 0.9 * 3_600_000,
  you_ms: 8.6 * 3_600_000,
  redo_ms: 1.2 * 3_600_000,
  orienting_ms: 2.3 * 3_600_000,
  approved_by_hand: 64,
  sessions: 31,
  start_tokens: 58_000,
  start_skills_tokens: 4_200,
  start_mcp_tokens: 1_100,
}

export const SAMPLE_CACHE: import('../lib/api').Cache = {
  hit_rate: 0.96,
  calls: 2140,
  rebuilds: 9,
  rebuild_tokens: 1_840_000,
  rebuild_usd: 14.2,
  by_cause: [
    { cause: 'break', count: 5, tokens: 1_210_000, usd: 9.4 },
    { cause: 'model_switch', count: 2, tokens: 390_000, usd: 3.0 },
    { cause: 'unknown', count: 2, tokens: 240_000, usd: 1.8 },
  ],
  compactions: 3,
  latest: [
    { session_id: 'demo', seq: 41, ts: '2026-11-06T09:10:00Z', cause: 'break', tokens: 212_000, usd: 1.65, gap_ms: 94 * 60_000, model: 'claude-opus-5-5', from_model: null, from_effort: null, effort: null, title: 'Migrate checkout to server actions' },
    { session_id: 'demo', seq: 12, ts: '2026-11-05T15:02:00Z', cause: 'model_switch', tokens: 180_000, usd: 1.4, gap_ms: 40_000, model: 'claude-sonnet-5-5', from_model: 'claude-opus-5-5', from_effort: null, effort: null, title: 'Fix the cart total rounding bug' },
  ],
}

export const SAMPLE_FIXES: import('../lib/api').Fix[] = [
  {
    id: 'slow:cargo test', kind: 'slow_command', title: '`cargo test` ran 41 times and took 1h 2m',
    detail: 'A typical run takes 1 min. It ran again with no file changed since the last run 12 times. A narrower or quieter form for everyday edits would give most of that back.',
    count: 41, ms_per_week: 50 * 60_000, usd_per_week: null, tokens_per_week: null, share: 0.07, fix: null, fix_where: 'AGENTS.md',
    prompt: 'Find a faster check for everyday edits and add it to AGENTS.md.', advice: null,
    examples: [{ session_id: 'demo', seq: 12, title: 'Fix the cart total rounding bug' }, { session_id: 'demo', seq: 40, title: 'Upgrade the Stripe client' }],
    weekly: [4, 9, 12, 16],
  },
  {
    id: 'approval:Bash(npm test:*)', kind: 'approval', title: 'You allowed `Bash(npm test:*)` by hand 38 times',
    detail: 'Each one held the agent until you answered, about 40 s at a time and 25 min in all (an estimate). You allowed every one.',
    count: 38, ms_per_week: 12 * 60_000, usd_per_week: null, tokens_per_week: null, share: 0.02,
    fix: JSON.stringify({ permissions: { allow: ['Bash(npm test:*)'] } }, null, 2), fix_where: 'Claude Code settings, under permissions.allow',
    prompt: 'Add `Bash(npm test:*)` to permissions.allow.', advice: null, examples: [{ session_id: 'demo', seq: 7, title: 'Add partial refunds' }], weekly: [6, 11, 9, 12],
  },
  {
    id: 'cache:break', kind: 'cache', title: 'The prompt cache was written again after a break 5 times',
    detail: '1.2M tokens were written to the cache a second time, about $9.40 at API list prices. This is how the cache works, not a fault; the cost is yours to weigh.',
    count: 5, ms_per_week: null, usd_per_week: 2.3, tokens_per_week: null, share: 0.06, fix: null, fix_where: null, prompt: null,
    advice: 'At the end of a task, start a fresh session rather than returning to a big one. Compacting before a long break also makes the return cheaper.',
    examples: [{ session_id: 'demo', seq: 41, title: 'Migrate checkout to server actions' }], weekly: [1, 0, 2, 2],
  },
]

const W = (count: number, ms: number, sessions: number, days: number) => ({ days, count, ms, usd: 0, agent_ms: 40 * 3_600_000, sessions })
export const SAMPLE_LEDGER: import('../lib/api').Ledger = {
  applied: [
    { id: 'slow:cargo test', kind: 'slow_command', title: '`cargo test` ran 41 times and took 1h 2m', state: 'applied', at: '2026-10-20T10:00:00Z',
      before: W(41, 3_720_000, 22, 28), after: W(9, 800_000, 14, 17), before_rate: 930_000, after_rate: 260_000, change: -0.72, enough: true, days_left: 0 },
    { id: 'approval:Bash(npm test:*)', kind: 'approval', title: 'You allowed `Bash(npm test:*)` by hand 38 times', state: 'applied', at: '2026-11-03T10:00:00Z',
      before: W(38, 1_500_000, 20, 28), after: W(0, 0, 3, 3), before_rate: 375_000, after_rate: 0, change: null, enough: false, days_left: 3.6 },
  ],
  dismissed: [{ id: 'cache:break', kind: 'cache', title: 'The prompt cache was written again after a break 5 times', state: 'dismissed', at: '2026-11-01T10:00:00Z' }],
}
