import { AGENTS, type AgentId } from './agents'
import type { Audit, Day, Overview, SessionDetail, Severity } from './api'
import { baseName, formatCompact, formatDate, formatHours, formatNumber, fromLocalDay, localDay, oneLine } from './format'
import { GROUPS, groupOf } from './kinds'
import { SEVERITY_RANK } from './severity'

/** Everything a card shows. Built from API data, drawn by `drawCard`. */
export type CardData = {
  eyebrow: string
  title: string
  subtitle: string
  stats: { label: string; value: string }[]
  /** Session: one colored segment per step, width by duration. */
  strip?: { color: string; weight: number }[]
  /** Week: agent time per day. */
  days?: { label: string; value: number }[]
  listTitle: string
  list: { label: string; value: string }[]
  risks: { severity: Severity; text: string }[]
}

const W = 1200
const H = 630

const weight = (ms: number | null) => 1 + Math.log10(1 + (ms ?? 0) / 250)

function topRisks(rs: { severity: Severity; title: string; n?: number }[]) {
  return [...rs]
    .sort((a, b) => SEVERITY_RANK[b.severity] - SEVERITY_RANK[a.severity] || (b.n ?? 1) - (a.n ?? 1))
    .slice(0, 3)
    .map((r) => ({ severity: r.severity, text: r.n && r.n > 1 ? `${r.title} ×${r.n}` : r.title }))
}

/** A card for one session. `redact` hides the title, project and paths. */
export function sessionCard(d: SessionDetail, redact: boolean): CardData {
  const x = d.session
  const changed = new Map<string, number>()
  for (const f of d.files) if (f.action !== 'read') changed.set(f.path, (changed.get(f.path) ?? 0) + 1)
  const root = x.project_root ?? ''
  const rel = (p: string) => (root && p.startsWith(root + '/') ? p.slice(root.length + 1) : baseName(p))
  const byRule = new Map<string, { severity: Severity; title: string; n: number }>()
  for (const f of d.findings) {
    const r = byRule.get(f.rule_id) ?? { severity: f.severity, title: f.title, n: 0 }
    r.n++
    byRule.set(f.rule_id, r)
  }
  const files = [...changed.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5)
  return {
    eyebrow: 'Session report',
    title: redact ? 'A coding session' : oneLine(x.title ?? x.first_prompt ?? 'Untitled session'),
    subtitle: [
      AGENTS[x.agent]?.label,
      redact ? null : x.project_root ? baseName(x.project_root) : null,
      x.model,
      x.started_at ? formatDate(x.started_at) : null,
    ]
      .filter(Boolean)
      .join('  ·  '),
    stats: [
      { label: 'Agent time', value: formatHours(x.active_ms) },
      { label: 'Steps', value: formatNumber(x.event_count) },
      { label: 'Tool calls', value: formatNumber(x.tool_call_count) },
      { label: 'Files changed', value: formatNumber(x.file_count) },
      { label: 'Steps that errored', value: formatNumber(x.error_count) },
    ],
    strip: d.events.map((e) => ({ color: GROUPS[groupOf(e.kind, e.category, e.is_error)].color, weight: weight(e.duration_ms) })),
    listTitle: 'Most changed files',
    list: files.map(([p, n], i) => ({ label: redact ? `file ${i + 1}` : rel(p), value: `${n}×` })),
    risks: topRisks([...byRule.values()]),
  }
}

/** A card for the last 7 days. */
export function weekCard(o: Overview, days: Day[], audit: Audit | null, agent: AgentId | '', redact: boolean): CardData {
  const byDay = new Map(days.map((d) => [d.day, d.active_ms]))
  const week = Array.from({ length: 7 }, (_, i) => {
    const d = new Date()
    d.setDate(d.getDate() - (6 - i))
    const key = localDay(d)
    return { label: fromLocalDay(key).toLocaleDateString(undefined, { weekday: 'short' }), value: byDay.get(key) ?? 0 }
  })
  const end = new Date()
  const start = new Date()
  start.setDate(end.getDate() - 6)
  const fmt = (d: Date) => d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
  const s = o.stats
  return {
    eyebrow: 'Weekly report',
    title: `${formatHours(s.active_ms)} of agent time`,
    subtitle: [`${fmt(start)} – ${fmt(end)}`, agent ? AGENTS[agent].label : 'All agents'].join('  ·  '),
    stats: [
      { label: 'Sessions', value: formatNumber(s.sessions) },
      { label: 'Prompts', value: formatNumber(s.prompts) },
      { label: 'Tool calls', value: formatCompact(s.tool_calls) },
      { label: 'Files changed', value: formatNumber(s.files_changed) },
      { label: 'Risks found', value: formatNumber(s.findings) },
    ],
    days: week,
    listTitle: 'Projects',
    list: o.projects.slice(0, 5).map((p, i) => ({
      label: redact ? `project ${i + 1}` : p.path ? baseName(p.path) : 'Unknown folder',
      value: formatHours(p.active_ms),
    })),
    risks: topRisks((audit?.rules ?? []).map((r) => ({ severity: r.severity, title: r.title, n: r.count }))),
  }
}

/** Resolve `var(--x)` against the page so the card uses the real tokens. */
function color(c: string) {
  const m = c.match(/^var\((--[\w-]+)\)$/)
  return m ? getComputedStyle(document.documentElement).getPropertyValue(m[1]).trim() : c
}

function fit(ctx: CanvasRenderingContext2D, text: string, max: number) {
  if (ctx.measureText(text).width <= max) return text
  let lo = 0
  let hi = text.length
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1
    if (ctx.measureText(text.slice(0, mid) + '…').width <= max) lo = mid
    else hi = mid - 1
  }
  return text.slice(0, lo) + '…'
}

function rrect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath()
  ctx.roundRect(x, y, w, h, r)
  ctx.fill()
}

const SEV_COLOR: Record<Severity, string> = {
  low: '--text-3',
  medium: '--status-warning',
  high: '--status-serious',
  critical: '--status-critical',
}

/** Draw a 1200×630 card (social image size) at 2× for sharp text. */
export async function drawCard(canvas: HTMLCanvasElement, c: CardData) {
  await document.fonts.ready
  const scale = 2
  canvas.width = W * scale
  canvas.height = H * scale
  const ctx = canvas.getContext('2d')!
  ctx.scale(scale, scale)
  const v = (name: string) => color(`var(${name})`)
  const sans = (size: number, weight = 420) => `${weight} ${size}px "Inter Variable", system-ui, sans-serif`
  const mono = (size: number) => `400 ${size}px "JetBrains Mono Variable", ui-monospace, monospace`
  const PAD = 56

  // Background and frame
  ctx.fillStyle = v('--bg-0')
  ctx.fillRect(0, 0, W, H)
  ctx.fillStyle = v('--bg-1')
  rrect(ctx, 24, 24, W - 48, H - 48, 20)

  // Header
  ctx.textBaseline = 'alphabetic'
  ctx.fillStyle = v('--text-3')
  ctx.font = sans(15, 560)
  ctx.fillText(c.eyebrow.toUpperCase(), PAD, 82)
  ctx.fillStyle = v('--text-1')
  ctx.font = sans(40, 640)
  ctx.fillText(fit(ctx, c.title, W - PAD * 2), PAD, 132)
  ctx.fillStyle = v('--text-2')
  ctx.font = sans(18)
  ctx.fillText(fit(ctx, c.subtitle, W - PAD * 2), PAD, 166)

  // Stats row
  const colW = (W - PAD * 2) / c.stats.length
  c.stats.forEach((s, i) => {
    const x = PAD + i * colW
    ctx.fillStyle = v('--text-3')
    ctx.font = sans(15, 520)
    ctx.fillText(s.label, x, 222)
    ctx.fillStyle = v('--text-1')
    ctx.font = sans(36, 640)
    ctx.fillText(s.value, x, 264)
  })

  // Chart band
  const chartY = 300
  const chartH = 70
  if (c.strip?.length) {
    const total = c.strip.reduce((n, s) => n + s.weight, 0)
    const gap = c.strip.length > 200 ? 0 : 1.5
    const avail = W - PAD * 2 - gap * (c.strip.length - 1)
    let x = PAD
    for (const s of c.strip) {
      const w = Math.max(0.6, (s.weight / total) * avail)
      ctx.fillStyle = color(s.color)
      ctx.fillRect(x, chartY + 18, w, chartH - 30)
      x += w + gap
    }
  } else if (c.days?.length) {
    const max = Math.max(1, ...c.days.map((d) => d.value))
    const bw = (W - PAD * 2) / c.days.length
    c.days.forEach((d, i) => {
      const h = d.value ? Math.max(4, (d.value / max) * (chartH - 22)) : 2
      ctx.fillStyle = d.value ? v('--heat-3') : v('--line-2')
      rrect(ctx, PAD + i * bw + 6, chartY + chartH - 22 - h, bw - 12, h, 4)
      ctx.fillStyle = v('--text-3')
      ctx.font = sans(13)
      ctx.fillText(d.label, PAD + i * bw + 6, chartY + chartH - 2)
    })
  }

  // Lists
  const listY = 412
  const half = (W - PAD * 2 - 40) / 2
  const heading = (t: string, x: number) => {
    ctx.fillStyle = v('--text-3')
    ctx.font = sans(14, 560)
    ctx.fillText(t.toUpperCase(), x, listY)
  }
  heading(c.listTitle, PAD)
  c.list.slice(0, 4).forEach((r, i) => {
    const y = listY + 32 + i * 28
    ctx.fillStyle = v('--text-1')
    ctx.font = mono(15)
    ctx.fillText(fit(ctx, r.label, half - 80), PAD, y)
    ctx.fillStyle = v('--text-2')
    ctx.font = sans(15)
    ctx.textAlign = 'right'
    ctx.fillText(r.value, PAD + half, y)
    ctx.textAlign = 'left'
  })
  if (!c.list.length) {
    ctx.fillStyle = v('--text-3')
    ctx.font = sans(15)
    ctx.fillText('None', PAD, listY + 32)
  }

  const rx = PAD + half + 40
  heading('Risks', rx)
  if (!c.risks.length) {
    ctx.fillStyle = v('--text-2')
    ctx.font = sans(15)
    ctx.fillText('No medium or higher risks found', rx, listY + 32)
  }
  c.risks.forEach((r, i) => {
    const y = listY + 32 + i * 28
    ctx.fillStyle = v(SEV_COLOR[r.severity])
    rrect(ctx, rx, y - 12, 10, 10, 3)
    ctx.fillStyle = v('--text-2')
    ctx.font = sans(13, 600)
    const word = r.severity.toUpperCase()
    ctx.fillText(word, rx + 18, y - 2)
    const ww = ctx.measureText(word).width
    ctx.fillStyle = v('--text-1')
    ctx.font = sans(15)
    ctx.fillText(fit(ctx, r.text, half - ww - 30), rx + 18 + ww + 10, y - 1)
  })

  // Footer: the logo mark and the name
  const fy = H - 52
  ctx.fillStyle = v('--bg-3')
  rrect(ctx, PAD, fy - 18, 24, 24, 6)
  ctx.strokeStyle = v('--text-1')
  ctx.lineWidth = 2
  ctx.beginPath()
  ctx.arc(PAD + 12, fy - 6, 5.5, 0, Math.PI * 2)
  ctx.stroke()
  ctx.fillStyle = v('--rec')
  ctx.beginPath()
  ctx.arc(PAD + 19, fy - 13, 3, 0, Math.PI * 2)
  ctx.fill()
  ctx.fillStyle = v('--text-1')
  ctx.font = sans(17, 650)
  ctx.fillText('agentcam', PAD + 34, fy)
  ctx.fillStyle = v('--text-3')
  ctx.font = sans(14)
  ctx.textAlign = 'right'
  ctx.fillText('github.com/chauhanswapnil/agentcam', W - PAD, fy)
  ctx.textAlign = 'left'
}
