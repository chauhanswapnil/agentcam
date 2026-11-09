import { useCallback, useEffect, useState } from 'react'
import type { AgentId } from './agents'
import type { EventKind, ToolCategory } from './kinds'

/** Mirrors the JSON types in agentcam-core `query.rs` and the server `api.rs`. */
export type Stats = {
  sessions: number
  active_ms: number
  events: number
  prompts: number
  tool_calls: number
  commands: number
  errors: number
  files_changed: number
  files_created: number
  projects: number
  findings: number
  serious_findings: number
  cost_usd: number
  unpriced_calls: number
  wasted_ms: number
  commits: number
}

export type Project = {
  path: string
  sessions: number
  active_ms: number
  events: number
  last_active: string | null
  cost_usd: number
  agents: AgentId[]
  weekly_ms: number[]
}

export type Overview = { since: string | null; stats: Stats; previous: Stats | null; projects: Project[] }

export type Day = { day: string; sessions: number; active_ms: number; events: number; tool_calls: number; cost_usd: number }

export type SessionRow = {
  id: string
  agent: AgentId
  title: string | null
  first_prompt: string | null
  project_path: string | null
  project_root: string | null
  git_branch: string | null
  model: string | null
  parent_id: string | null
  started_at: string | null
  ended_at: string | null
  active_ms: number
  event_count: number
  tool_call_count: number
  error_count: number
  file_count: number
  input_tokens: number
  output_tokens: number
  cache_read_tokens: number
  cost_usd: number | null
  subagents: number
  finding_count: number
  max_severity: Severity | null
  est_cost_usd: number | null
  cost_unpriced: boolean
  peak_context: number
  wasted_ms: number
  pattern_count: number
  commit_count: number
  entrypoint: string | null
  reasoning_tokens?: number
  service_tier?: string | null
}

export type SessionPage = { total: number; items: SessionRow[] }

export type EventRow = {
  seq: number
  ts: string | null
  kind: EventKind
  tool_name: string | null
  category: ToolCategory | null
  title: string
  is_error: boolean
  duration_ms: number | null
  /** Start of the text for prompts, responses, thinking and notices. */
  text: string | null
  context_tokens: number | null
  tokens_out: number
  cost_usd: number | null
  wasted: boolean
  approval: string | null
  via: string | null
}

export type Pattern = {
  kind: 'retry_loop' | 'error_streak' | 'revert' | 'edit_churn'
  start_seq: number
  end_seq: number
  title: string
  wasted_ms: number
  wasted_tokens: number
}

export type CommitRow = {
  session_id: string
  seq: number
  ts: string | null
  hash: string | null
  branch: string | null
  message: string | null
  agent: AgentId
  project_root: string | null
  session_title: string | null
}

export type FileRow = { seq: number; path: string; action: 'read' | 'edit' | 'create' | 'delete' }

export type Severity = 'low' | 'medium' | 'high' | 'critical'

export type FindingRow = {
  seq: number
  rule_id: string
  title: string
  severity: Severity
  excerpt: string
  /** Sequence and outside-content findings: the earlier step the chain started from. */
  related_seq: number | null
  /** `reviewed` or `muted` once someone has looked. */
  review?: 'reviewed' | 'muted' | null
}

export type SessionDetail = {
  session: SessionRow
  events: EventRow[]
  files: FileRow[]
  findings: FindingRow[]
  patterns: Pattern[]
  commits: CommitRow[]
  subagents: SessionRow[]
  lasted: Lasted | null
  turns: TurnRow[]
  prs: PrRow[]
  follow: SessionFollow | null
  plan?: SessionPlan | null
}

/** Mirrors `plans.rs` in agentcam-core. */
export type PlanTask = { text: string; status: 'pending' | 'in_progress' | 'completed' }
export type SessionPlan = {
  snapshots: { seq: number; tasks: PlanTask[] }[]
  plans: { seq: number; text: string }[]
}

export type RuleSummary = { rule_id: string; title: string; severity: Severity; count: number; sessions: number; last_ts: string | null; muted: boolean }

export type FindingHit = FindingRow & {
  session_id: string
  ts: string | null
  step_title: string
  agent: AgentId
  session_title: string | null
  project_root: string | null
}

export type Audit = { rules: RuleSummary[]; findings: FindingHit[]; reviewed: number }

export type ToolStat = {
  tool_name: string
  category: ToolCategory | null
  calls: number
  errors: number
  total_ms: number
  sessions: number
  last_ts: string | null
}

export type StepRef = {
  session_id: string
  seq: number
  ts: string | null
  title: string
  is_error: boolean
  duration_ms: number | null
  action: 'read' | 'edit' | 'create' | 'delete' | null
  agent: AgentId
  session_title: string | null
}

export type FileStat = {
  path: string
  reads: number
  edits: number
  creates: number
  deletes: number
  sessions: number
  last_ts: string | null
}

export type IndexStatus = {
  indexing: boolean
  last_report: { files_seen: number; files_parsed: number; events_written: number; elapsed_ms: number } | null
  last_error: string | null
  generation: number
  /** False until the index holds a session: a first run. */
  has_sessions: boolean
  /** The log folders agentcam reads, and whether each exists now. */
  folders: { agent: string; path: string; exists: boolean }[]
}

export type Hit = {
  session_id: string
  seq: number
  ts: string | null
  kind: EventKind
  category: ToolCategory | null
  tool_name: string | null
  is_error: boolean
  title: string
  /** Matched words are wrapped in \u0001 ... \u0002. */
  snippet: string
  agent: AgentId
  session_title: string | null
  project_root: string | null
}

export type Filters = { range?: string; agent?: string; project?: string; day?: string }

export function query(params: Record<string, string | number | undefined>) {
  const q = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== '') q.set(k, String(v))
  const s = q.toString()
  return s ? `?${s}` : ''
}

export async function getJson<T>(url: string): Promise<T> {
  const r = await fetch(url)
  if (!r.ok) throw new Error(r.status === 404 ? 'Not found' : `HTTP ${r.status}`)
  return r.json() as Promise<T>
}

type State<T> = { data: T | null; error: string | null; loading: boolean }

/**
 * Fetch JSON when `url` or `refresh` changes. Keeps the last data while a
 * new request is in flight, so views don't flash empty.
 */
export function useApi<T>(url: string | null, refresh = 0): State<T> {
  const key = url ? `${url}#${refresh}` : null
  const [got, setGot] = useState<{ key: string | null; data: T | null; error: string | null }>({
    key: null,
    data: null,
    error: null,
  })
  useEffect(() => {
    if (!url || !key) return
    let live = true
    getJson<T>(url)
      .then((data) => live && setGot({ key, data, error: null }))
      .catch((e: Error) => live && setGot((g) => ({ key, data: g.data, error: e.message })))
    return () => {
      live = false
    }
  }, [url, key])
  return { data: got.data, error: got.error, loading: !!key && got.key !== key }
}

/**
 * Index status. Polls while indexing; `generation` goes up each time a run
 * finishes, which callers pass as `refresh` to refetch.
 */
export function useIndexStatus() {
  const [status, setStatus] = useState<IndexStatus | null>(null)
  const load = useCallback(() => getJson<IndexStatus>('/api/status').then(setStatus).catch(() => {}), [])
  useEffect(() => {
    load()
  }, [load])
  // Poll fast while indexing, slowly otherwise: the server re-indexes on
  // its own when agents write logs, and `generation` tells us to refetch.
  useEffect(() => {
    const t = window.setInterval(load, status?.indexing ? 700 : 3000)
    return () => window.clearInterval(t)
  }, [status?.indexing, load])
  const reindex = useCallback(async () => {
    await fetch('/api/index', { method: 'POST' })
    load()
  }, [load])
  return { status, reindex }
}

/** `value`, but only after it has stopped changing for `ms`. */
export function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value)
  useEffect(() => {
    const t = window.setTimeout(() => setV(value), ms)
    return () => window.clearTimeout(t)
  }, [value, ms])
  return v
}

export type GuardRule = {
  id: string
  tool: string
  pattern: string
  reason: string
  created: string | null
  from_session: string | null
  from_seq: number | null
}

export type HookSetup = { agent: AgentId; settings_path: string | null; installed: boolean; snippet: string; note: string }

export type GuardInfo = { rules: GuardRule[]; path: string | null; hooks: HookSetup[] }

export type RuleDef = {
  id: string
  title: string
  severity: Severity
  field: string
  pattern: string | null
  kinds: string[]
  categories: string[]
  actions: string[]
  secret: boolean
  outside_project: boolean
  tags?: string[]
  unless?: string
  when_tagged?: string[]
  sequence?: { tags?: string[]; rules?: string[] }[]
  within?: number
  outside_content?: string[]
}

export type RulesInfo = { rules: RuleDef[]; builtin_ids: string[]; user_yaml: string; user_path: string | null }

export type GuardTest = { valid: boolean; error: string | null; matches: number; examples: StepRef[] }

/** Send JSON to the API. Throws with the server's message on failure. */
export async function sendJson<T>(method: 'POST' | 'DELETE', url: string, body?: unknown): Promise<T> {
  const r = await fetch(url, {
    method,
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!r.ok) {
    const msg = await r.json().then((j: { error?: string }) => j.error).catch(() => null)
    throw new Error(msg ?? `HTTP ${r.status}`)
  }
  return r.json() as Promise<T>
}

/** Mirrors `live.rs` in agentcam-core. */
export type LiveStep = {
  session_id: string
  seq: number
  ts: string | null
  kind: EventKind
  category: ToolCategory | null
  tool_name: string | null
  title: string
  is_error: boolean
}

export type LiveState = 'working' | 'waiting' | 'quiet'

export type LiveSession = {
  session: SessionRow
  last_at: string
  idle_ms: number
  state: LiveState
  context_tokens: number | null
  active_subagents: number
  /** Tool calls in a row that errored, counting back from the latest. */
  error_streak?: number
  /** Session plus subagents at API list prices. */
  cost_usd?: number | null
  recent: LiveStep[]
}

export type LimitWindow = {
  agent: AgentId
  window: string
  window_minutes: number | null
  used_percent: number
  resets_at: string | null
  at: string
  plan: string | null
  session_id: string
  expired: boolean
}

export type LimitSeries = { agent: AgentId; window: string; points: { ts: string; used_percent: number }[] }

export type LimitHit = {
  agent: AgentId
  window: string
  ts: string
  resets_at: string | null
  resets_text: string | null
  session_id: string
  seq: number | null
  title: string | null
  project_root: string | null
}

/** Mirrors `quota.rs`: Claude Code's usage, estimated from logged tokens. */
export type QuotaTotals = {
  calls: number
  input_tokens: number
  output_tokens: number
  cache_read_tokens: number
  cache_write_tokens: number
  total_tokens: number
  cost_usd: number
  unpriced_calls: number
}
export type QuotaBlock = QuotaTotals & { start: string; end: string; first_at: string; last_at: string; active: boolean }
export type QuotaPace = {
  tokens_per_minute: number
  cost_per_hour: number
  projected_tokens: number
  projected_cost_usd: number
  remaining_ms: number
}
export type ClaudeQuota = {
  estimate: true
  block_minutes: number
  active: QuotaBlock | null
  pace: QuotaPace | null
  peak: QuotaBlock | null
  blocks: QuotaBlock[]
  week: QuotaTotals
}

export type Limits = {
  windows: LimitWindow[]
  series: LimitSeries[]
  hits: LimitHit[]
  hits_30d: number
  claude?: ClaudeQuota | null
}

/** Mirrors `survival.rs`: lines agents added and where they are now. */
export type Lasted = { added: number; kept: number; reworked: number; changed: number }

export type LastedReport = {
  settled: Lasted
  recent: Lasted
  ages: { label: string; counts: Lasted }[]
  weeks: { week: string; counts: Lasted }[]
  by_agent: { agent: AgentId; model: string | null; sessions: number; counts: Lasted }[]
  by_project: { project: string | null; sessions: number; counts: Lasted }[]
  files: { path: string; project: string | null; edits: number; counts: Lasted }[]
}

/** Mirrors `turns.rs`: one prompt you typed and the work until the next. */
export type TurnRow = {
  n: number
  seq: number
  end_seq: number
  prompt: string
  started_at: string | null
  ended_at: string | null
  duration_ms: number | null
  wait_ms: number | null
  steps: number
  tool_calls: number
  errors: number
  cost_usd: number | null
  thinking_ms: number
  model: string | null
  effort: string | null
  mode: string | null
  approval_policy: string | null
  sandbox: string | null
}

export type TurnGroup = { key: string | null; turns: number; median_ms: number; median_steps: number; avg_cost_usd: number | null }

export type TurnStats = {
  turns: number
  median_ms: number
  p90_ms: number
  median_steps: number
  median_wait_ms: number | null
  lengths: number[]
  by_model: TurnGroup[]
  by_effort: TurnGroup[]
  by_mode: TurnGroup[]
  longest: { session_id: string; agent: AgentId; title: string | null; turn: TurnRow }[]
}

export type PrRow = {
  session_id: string
  seq: number
  ts: string | null
  number: number | null
  url: string
  repo: string | null
  agent: AgentId
  title: string | null
}

export type Source = { kind: 'skill' | 'plugin' | 'app' | 'mcp'; name: string; steps: number; errors: number; sessions: number; last_ts: string | null }

export type Approvals = {
  counts: Record<string, number>
  tools: { tool_name: string; counts: Record<string, number> }[]
  stopped: { session_id: string; seq: number; ts: string | null; tool_name: string | null; title: string; approval: string; agent: AgentId }[]
  suggestions: { rule: string; by_you: number; by_auto: number; last_ts: string | null }[]
}

export type Thought = {
  session_id: string
  seq: number
  ts: string | null
  text: string
  duration_ms: number | null
  agent: AgentId
  model: string | null
  title: string | null
  project_root: string | null
}

export type Thinking = {
  blocks: number
  total_ms: number
  by_model: { model: string | null; blocks: number; total_ms: number; timed: number }[]
  items: Thought[]
}

/** Mirrors `follow.rs`. */
export type FollowRule = {
  id: string
  kind: 'checks' | 'package_manager' | 'dont_edit' | 'dont_run'
  commands?: string[]
  when?: string | null
  tool?: string
  path?: string
  command?: string
  source: string
  file: string
}
export type Outcome = { rule_id: string; status: 'followed' | 'not_followed' | 'not_applicable'; detail: string | null; seq: number | null; session_id: string }
export type SessionFollow = { files: string[]; rules: FollowRule[]; outcomes: Outcome[] }
export type ProjectFollow = {
  project: string
  files: string[]
  sessions: number
  rules: {
    rule: FollowRule
    followed: number
    not_followed: number
    examples: { session_id: string; seq: number | null; title: string | null; ts: string | null; detail: string | null }[]
  }[]
}

/** Mirrors `skills.rs`. */
export type SkillRow = {
  name: string
  description: string | null
  sources: string[]
  file_chars: number | null
  uses: Record<string, number>
  sessions: number
  steps: number
  errored_steps: number
  avg_loaded_chars: number | null
  listed_in: number
  last_used: string | null
}
export type SkillsReport = {
  skills: SkillRow[]
  listing_sessions: number
  avg_listed: number
  avg_listing_chars: number
  recent: { session_id: string; seq: number; ts: string | null; skill: string; trigger: string; args: string | null; agent: AgentId; title: string | null; project_root: string | null }[]
}

/** Mirrors `compare.rs`. */
export type CompareDim = 'model' | 'agent' | 'agent_version' | 'effort' | 'mode' | 'skill' | 'date'
export type Cohort = {
  value: string
  turns: number
  sessions: number
  first: string | null
  last: string | null
  median_turn_ms: number
  median_steps: number
  tool_calls_per_turn: number
  avg_cost_usd: number | null
  errored_per_100_steps: number
  retry_pct: number
  stopped_per_100_calls: number
  thinking_ms_per_turn: number
  lines_added: number
  lasted_pct: number | null
}
export type Comparison = { dim: CompareDim; a: Cohort; b: Cohort }
export type CompareOptions = {
  dims: { dim: CompareDim; values: [string, number][] }[]
  suggestions: { title: string; dim: CompareDim; a: string; b: string; at: string | null; project: string | null; turns_a: number; turns_b: number }[]
}
