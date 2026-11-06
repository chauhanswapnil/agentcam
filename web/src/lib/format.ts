const nf = new Intl.NumberFormat('en-US')
const compact = new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 })

export const formatNumber = (n: number) => nf.format(n)
export const formatCompact = (n: number) => compact.format(n)

/** "$1.24", "$1,204", "<$0.01". */
export function formatCost(usd: number): string {
  if (usd > 0 && usd < 0.01) return '<$0.01'
  return usd >= 100 ? '$' + Math.round(usd).toLocaleString('en-US') : '$' + usd.toFixed(2)
}

/** 2400 -> "2.4s", 95_000 -> "1m 35s", 7_380_000 -> "2h 3m". */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`
  const s = Math.round(ms / 1000)
  if (s < 60) return `${(ms / 1000).toFixed(s < 10 ? 1 : 0)}s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m}m ${s % 60}s`
  return `${Math.floor(m / 60)}h ${m % 60}m`
}

/** Last path segment, for showing a project by its folder name. */
export const baseName = (p: string) => p.replace(/[/\\]+$/, '').split(/[/\\]/).pop() ?? p

/** Agent time for headlines: "18h 30m", "42m", "35s". */
export function formatHours(ms: number): string {
  const m = Math.round(ms / 60_000)
  if (m < 1) return `${Math.round(ms / 1000)}s`
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  return m % 60 && h < 100 ? `${h}h ${m % 60}m` : `${h}h`
}

const timeFmt = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' })
const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short', month: 'short', day: 'numeric' })
const dateFmt = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', year: 'numeric' })

export const formatTime = (iso: string) => timeFmt.format(new Date(iso))
export const formatDate = (iso: string) => dateFmt.format(new Date(iso))

/** Local `YYYY-MM-DD` for a date. */
export function localDay(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`
}

/** Date from a local `YYYY-MM-DD`. */
export const fromLocalDay = (day: string) => {
  const [y, m, d] = day.split('-').map(Number)
  return new Date(y, m - 1, d)
}

/** "Today", "Yesterday", or "Wed, Nov 5" for a local day. */
export function dayLabel(day: string): string {
  const today = localDay(new Date())
  const y = new Date()
  y.setDate(y.getDate() - 1)
  if (day === today) return 'Today'
  if (day === localDay(y)) return 'Yesterday'
  return dayFmt.format(fromLocalDay(day))
}

/** "just now", "5m ago", "3h ago", "2d ago", then a date. */
export function formatAgo(iso: string): string {
  const s = (Date.now() - new Date(iso).getTime()) / 1000
  if (s < 60) return 'just now'
  if (s < 3600) return `${Math.floor(s / 60)}m ago`
  if (s < 86_400) return `${Math.floor(s / 3600)}h ago`
  if (s < 7 * 86_400) return `${Math.floor(s / 86_400)}d ago`
  return dateFmt.format(new Date(iso))
}

/** Change from `prev` to `cur` as "+12%" / "−8%", or null when there is no base. */
export function formatDelta(cur: number, prev: number): string | null {
  if (!prev) return null
  const pct = Math.round(((cur - prev) / prev) * 100)
  if (pct === 0) return 'same as'
  return `${pct > 0 ? '+' : '−'}${Math.abs(pct)}%`
}

/** First line of a prompt, without markup noise. */
export const oneLine = (s: string) => s.replace(/\s+/g, ' ').trim()
