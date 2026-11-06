import { useEffect, useRef, useSyncExternalStore } from 'react'
import type { Day, LiveSession, LiveState } from './api'
import { getJson } from './api'
import { baseName, formatCost, localDay, oneLine } from './format'

/** What to notify about. Everything lives in this browser's localStorage. */
export type AlertSettings = {
  /** The user switched alerts on (and the browser asked for permission). */
  enabled: boolean
  kinds: { waiting: boolean; finished: boolean; errors: boolean; cost: boolean }
  /** USD; null means off. */
  sessionCost: number | null
  dayCost: number | null
  /** Project roots that never notify. */
  muted: string[]
  quiet: { on: boolean; from: string; to: string }
}

export const ALERT_KINDS = [
  { key: 'waiting', label: 'Waiting for you', hint: 'A working session replied and may need you' },
  { key: 'finished', label: 'Done for now', hint: 'A working session stopped writing steps without a reply' },
  { key: 'errors', label: 'Steps erroring in a row', hint: `${4} or more tool calls in a row errored` },
  { key: 'cost', label: 'Cost passed a limit you set', hint: 'Per session or per day, at API list prices' },
] as const

/** Tool calls in a row that errored before an alert goes out (as patterns.rs counts a streak). */
export const STREAK = 4

const KEY = 'agentcam.alerts'
const FIRED = 'agentcam.alerts.fired'

export const DEFAULTS: AlertSettings = {
  enabled: false,
  kinds: { waiting: true, finished: false, errors: true, cost: true },
  sessionCost: null,
  dayCost: null,
  muted: [],
  quiet: { on: false, from: '22:00', to: '08:00' },
}

function read<T>(key: string, fallback: T): T {
  try {
    const raw = window.localStorage.getItem(key)
    return raw ? { ...fallback, ...JSON.parse(raw) } : fallback
  } catch {
    return fallback
  }
}

function write(key: string, value: unknown) {
  try {
    window.localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // Storage blocked (private window): settings last until reload.
  }
}

let current: AlertSettings = read(KEY, DEFAULTS)
const listeners = new Set<() => void>()

export function getAlertSettings(): AlertSettings {
  return current
}

export function setAlertSettings(patch: Partial<AlertSettings>) {
  current = { ...current, ...patch }
  write(KEY, current)
  listeners.forEach((l) => l())
}

/** The settings, re-rendering when any page changes them. */
export function useAlertSettings(): AlertSettings {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => current,
  )
}

export function notifySupported(): boolean {
  return typeof window !== 'undefined' && 'Notification' in window
}

/** Ask the browser for permission; only call from a click. */
export async function enableAlerts(): Promise<NotificationPermission | 'unsupported'> {
  if (!notifySupported()) return 'unsupported'
  const p = Notification.permission === 'default' ? await Notification.requestPermission() : Notification.permission
  setAlertSettings({ enabled: p === 'granted' })
  return p
}

const minutesOf = (hhmm: string) => {
  const [h, m] = hhmm.split(':').map(Number)
  return (h || 0) * 60 + (m || 0)
}

/** Quiet hours may wrap past midnight (22:00 to 08:00). */
export function inQuietHours(q: AlertSettings['quiet'], d: Date): boolean {
  if (!q.on) return false
  const now = d.getHours() * 60 + d.getMinutes()
  const from = minutesOf(q.from)
  const to = minutesOf(q.to)
  if (from === to) return false
  return from < to ? now >= from && now < to : now >= from || now < to
}

export type Prev = { state: LiveState; streak: number }
export type Alert = { kind: keyof AlertSettings['kinds']; id: string; title: string; body: string; tag: string }

const titleOf = (x: LiveSession) =>
  x.session.title ?? (x.session.first_prompt ? oneLine(x.session.first_prompt).slice(0, 80) : 'Untitled session')

/**
 * Alerts for what changed between two readings of `/api/live`. Pure, so
 * it can be tested and so quiet hours and mutes only gate the sending.
 */
export function changes(
  prev: Map<string, Prev>,
  next: LiveSession[],
  s: AlertSettings,
  fired: Record<string, boolean>,
): Alert[] {
  const out: Alert[] = []
  for (const x of next) {
    const id = x.session.id
    const p = prev.get(id)
    const where = x.session.project_root ? ` · ${baseName(x.session.project_root)}` : ''
    const title = titleOf(x)
    if (s.muted.includes(x.session.project_root ?? '')) continue
    if (p && s.kinds.waiting && p.state === 'working' && x.state === 'waiting') {
      out.push({ kind: 'waiting', id, title: 'Waiting for you', body: `${title}${where}`, tag: `${id}:waiting` })
    }
    if (p && s.kinds.finished && p.state === 'working' && x.state === 'quiet') {
      out.push({ kind: 'finished', id, title: 'Done for now', body: `${title}${where}`, tag: `${id}:finished` })
    }
    const streak = x.error_streak ?? 0
    if (p && s.kinds.errors && p.streak < STREAK && streak >= STREAK) {
      out.push({
        kind: 'errors',
        id,
        title: `${streak} steps in a row errored`,
        body: `Worth a look: ${title}${where}`,
        tag: `${id}:errors`,
      })
    }
    const cost = x.cost_usd ?? x.session.est_cost_usd
    const limit = s.sessionCost
    if (s.kinds.cost && limit != null && cost != null && cost >= limit && !fired[`s:${id}:${limit}`]) {
      out.push({
        kind: 'cost',
        id,
        title: `Session passed ${formatCost(limit)}`,
        body: `${formatCost(cost)} so far at API prices: ${title}${where}`,
        tag: `s:${id}:${limit}`,
      })
    }
  }
  return out
}

function readFired(): Record<string, boolean> {
  return read<Record<string, boolean>>(FIRED, {})
}

function markFired(tag: string) {
  const f = readFired()
  f[tag] = true
  // Keep the list short: cost tags are the only ones stored.
  const keys = Object.keys(f)
  if (keys.length > 200) keys.slice(0, keys.length - 200).forEach((k) => delete f[k])
  write(FIRED, f)
}

function send(a: Alert, onOpen: (id: string) => void) {
  try {
    const n = new Notification(a.title, { body: a.body, tag: a.tag, icon: '/favicon.svg' })
    n.onclick = () => {
      window.focus()
      if (a.id) onOpen(a.id)
      n.close()
    }
  } catch {
    // Some browsers only allow notifications from a service worker.
  }
}

/**
 * Watch live sessions and send browser notifications for the kinds the
 * user switched on. Mounted once, in the app shell.
 */
export function useAlerts(live: LiveSession[] | null, refresh: number, onOpen: (id: string) => void) {
  const s = useAlertSettings()
  const prev = useRef<Map<string, Prev> | null>(null)
  const open = useRef(onOpen)
  open.current = onOpen
  const allowed = () =>
    s.enabled && notifySupported() && Notification.permission === 'granted' && !inQuietHours(s.quiet, new Date())

  useEffect(() => {
    if (!live) return
    const before = prev.current
    prev.current = new Map(live.map((x) => [x.session.id, { state: x.state, streak: x.error_streak ?? 0 }]))
    // The first reading only sets the baseline.
    if (!before) return
    const fired = readFired()
    const found = changes(before, live, s, fired)
    for (const a of found) {
      if (a.kind === 'cost') markFired(a.tag)
      if (allowed()) send(a, open.current)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- runs per reading, settings read fresh
  }, [live])

  // Today's cost, checked whenever the index changes.
  useEffect(() => {
    const limit = s.dayCost
    if (!s.enabled || !s.kinds.cost || limit == null) return
    let gone = false
    getJson<Day[]>('/api/activity')
      .then((days) => {
        if (gone) return
        const today = localDay(new Date())
        const d = days.find((x) => x.day === today)
        const tag = `d:${today}:${limit}`
        if (d && d.cost_usd >= limit && !readFired()[tag]) {
          markFired(tag)
          if (allowed())
            send({ kind: 'cost', id: '', title: `Today passed ${formatCost(limit)}`, body: `${formatCost(d.cost_usd)} of agent work at API prices`, tag }, open.current)
        }
      })
      .catch(() => {})
    return () => {
      gone = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refetch per index run
  }, [refresh, s.enabled, s.kinds.cost, s.dayCost])
}

/** Sessions that replied and may be waiting for you, leaving out muted projects. */
export function needsYou(live: LiveSession[] | null, s: AlertSettings): number {
  return live?.filter((x) => x.state === 'waiting' && !s.muted.includes(x.session.project_root ?? '')).length ?? 0
}
