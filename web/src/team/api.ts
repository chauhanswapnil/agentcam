import { useEffect, useState } from 'react'
import type { Lasted } from '../lib/api'

/** The viewer token for the team UI, kept in this browser only. */
const KEY = 'agentcam-team-token'

export function getToken(): string | null {
  try {
    return window.localStorage.getItem(KEY)
  } catch {
    return null
  }
}

export function setToken(t: string | null) {
  try {
    if (t) window.localStorage.setItem(KEY, t)
    else window.localStorage.removeItem(KEY)
  } catch {
    // Private mode: the token lasts for this page only.
  }
}

export class AuthError extends Error {}

async function call<T>(method: 'GET' | 'POST', url: string, body?: unknown): Promise<T> {
  const t = getToken()
  const r = await fetch(url, {
    method,
    headers: {
      ...(t ? { authorization: `Bearer ${t}` } : {}),
      ...(body === undefined ? {} : { 'content-type': 'application/json' }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (r.status === 401) throw new AuthError('Sign in again')
  if (!r.ok) {
    const msg = await r.json().then((j: { error?: string }) => j.error).catch(() => null)
    throw new Error(msg ?? `HTTP ${r.status}`)
  }
  return r.json() as Promise<T>
}

export const teamGet = <T>(url: string) => call<T>('GET', url)
export const teamPost = <T>(url: string, body: unknown) => call<T>('POST', url, body)

type State<T> = { data: T | null; error: string | null; auth: boolean }

/** Fetch from the team API when `url` or `refresh` changes. */
export function useTeamApi<T>(url: string, refresh = 0): State<T> {
  const [st, setSt] = useState<State<T> & { key: string | null }>({ data: null, error: null, auth: false, key: null })
  const key = `${url}#${refresh}`
  useEffect(() => {
    let live = true
    teamGet<T>(url)
      .then((data) => live && setSt({ data, error: null, auth: false, key }))
      .catch((e: Error) => live && setSt((s) => ({ ...s, error: e.message, auth: e instanceof AuthError, key })))
    return () => {
      live = false
    }
  }, [url, key])
  return st
}

/** Mirrors `views.rs` in agentcam-team. */
export type Sum = {
  sessions: number
  machines: number
  active_ms: number
  retry_ms: number
  steps: number
  errored_steps: number
  cost_usd: number
  unpriced: number
  commits: number
  prs: number
  lasted: Lasted
  followed: number
  not_followed: number
  findings: number
}

export type Insights = {
  days: number | null
  totals: Sum
  previous: Sum | null
  weeks: ({ week: string } & Sum)[]
  by_team: ({ name: string } & Sum)[]
  by_project: ({ name: string } & Sum)[]
  by_agent_model: ({ agent: 'claude-code' | 'codex'; model: string | null } & Sum)[]
}

export type Cohort = {
  agent: 'claude-code' | 'codex'
  label: string
  sessions: number
  machines: number
  median_active_ms: number | null
  median_cost_usd: number | null
  errors_per_100: number | null
  retry_share: number | null
  kept_share: number | null
  followed_share: number | null
}

export type Value = {
  days: number | null
  totals: Sum
  cost_per_commit: number | null
  cost_per_pr: number | null
  kept_lines_per_dollar: number | null
  seats: { price_usd: number | null; seats: number; seat_cost_usd: number | null; api_value_usd: number }
  weeks: string[]
  adoption: { team: string; machines: number[] }[]
  by_model: Cohort[]
  by_version: Cohort[]
}

export type Governance = {
  days: number | null
  by_severity: Record<string, number>
  rules: { rule_id: string; severity: string; count: number; sessions: number; machines: number; projects: number }[]
  latest: {
    ts: string | null
    rule_id: string
    severity: string
    project: string | null
    team: string
    machine: string
    step: number
    excerpt: string
  }[]
  show_people: boolean
}

export type Fleet = {
  machines: {
    label: string
    team: string
    os: string | null
    agentcam_version: string | null
    joined_at: string
    last_sync: string | null
    agents: [string, string | null][]
    flags: string[]
    sessions: number
  }[]
  versions: { agent: string; version: string | null; machines: number; sessions: number }[]
  items: { kind: string; agent: string; name: string; detail: string | null; machines: number; tags: [string, number][] }[]
  flags: { id: string; agent: string; machines: number }[]
  show_people: boolean
}

export type Baseline = {
  require: string[]
  forbid: string[]
  forbid_tags: string[]
  mcp_allowlist: string[] | null
  min_versions: Record<string, string>
}

export type BaselineView = {
  baseline: Baseline
  set: boolean
  suggested: Baseline
  flags: [string, string][]
  tags: [string, string][]
  machines: number
  matching: number
  by_rule: [string, number][]
  drift: { machine: string; team: string; last_sync: string | null; differences: { rule: string; title: string; since: string | null }[] }[]
  changes: { ts: string; machine: string; what: string; change: 'added' | 'removed' }[]
}

export type SiemSettings = {
  webhook: { url: string; format: 'ndjson' | 'ocsf' | 'splunk_hec' } | null
  token_set: boolean
  last: { ts: string; ok: boolean; sent: number; message: string } | null
}

export type Team = { id: number; name: string; machines: number }

export type Settings = {
  org_name: string
  seat_price_usd: number | null
  show_people: boolean
  teams: Team[]
  log: { ts: string; actor: string; action: string; detail: string | null }[]
}

export type Me = { name: string; role: 'admin' | 'viewer'; org: string }

/** 0.734 -> "73%". */
export const share = (x: number | null | undefined) => (x == null ? '–' : `${Math.round(x * 100)}%`)
