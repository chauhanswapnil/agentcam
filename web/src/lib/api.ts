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
}

export type Project = {
  path: string
  sessions: number
  active_ms: number
  events: number
  last_active: string | null
  agents: AgentId[]
  weekly_ms: number[]
}

export type Overview = { since: string | null; stats: Stats; previous: Stats | null; projects: Project[] }

export type Day = { day: string; sessions: number; active_ms: number; events: number; tool_calls: number }

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
}

export type FileRow = { seq: number; path: string; action: 'read' | 'edit' | 'create' | 'delete' }

export type SessionDetail = { session: SessionRow; events: EventRow[]; files: FileRow[]; subagents: SessionRow[] }

export type IndexStatus = {
  indexing: boolean
  last_report: { files_seen: number; files_parsed: number; events_written: number; elapsed_ms: number } | null
  last_error: string | null
  generation: number
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
  useEffect(() => {
    if (!status?.indexing) return
    const t = window.setInterval(load, 700)
    return () => window.clearInterval(t)
  }, [status?.indexing, load])
  const reindex = useCallback(async () => {
    await fetch('/api/index', { method: 'POST' })
    load()
  }, [load])
  return { status, reindex }
}
