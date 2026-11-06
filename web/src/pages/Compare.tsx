import { GitCompareArrows, Lightbulb, TriangleAlert } from 'lucide-react'
import { useLocation, useSearch } from 'wouter'
import type { Cohort, CompareDim, CompareOptions, Comparison } from '../lib/api'
import { query, useApi, useIndexStatus } from '../lib/api'
import { baseName, formatCost, formatDate, formatDuration, formatNumber } from '../lib/format'
import { modeLabel } from '../lib/turns'
import { EmptyState, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import v from './Views.module.css'
import s from './Compare.module.css'

const RANGES = [
  { value: '30d', label: '30 days' },
  { value: '90d', label: '90 days' },
  { value: 'all', label: 'All time' },
]
const DIMS: { value: CompareDim; label: string }[] = [
  { value: 'model', label: 'Model' },
  { value: 'agent_version', label: 'Agent version' },
  { value: 'effort', label: 'Effort' },
  { value: 'mode', label: 'Mode' },
  { value: 'skill', label: 'Skill' },
  { value: 'agent', label: 'Agent' },
]

const short = (ms: number) => formatDuration(ms).replace(/ \d+s$/, '')

/** How each number is shown and what its change is called. A change is
 * described, never scored: "shorter", not "better". */
type Metric = {
  label: string
  get: (c: Cohort) => number | null
  fmt: (n: number) => string
  /** Words for down / up. */
  words: [string, string]
  /** Percentages compare in points, the rest relative. */
  points?: boolean
}
const METRICS: Metric[] = [
  { label: 'Typical turn length', get: (c) => c.median_turn_ms, fmt: formatDuration, words: ['shorter', 'longer'] },
  { label: 'Steps per turn (typical)', get: (c) => c.median_steps, fmt: (n) => n.toFixed(n % 1 ? 1 : 0), words: ['fewer', 'more'] },
  { label: 'Tool calls per turn', get: (c) => c.tool_calls_per_turn, fmt: (n) => n.toFixed(1), words: ['fewer', 'more'] },
  { label: 'Cost per turn', get: (c) => c.avg_cost_usd, fmt: formatCost, words: ['lower', 'higher'] },
  { label: 'Thinking per turn', get: (c) => c.thinking_ms_per_turn || null, fmt: short, words: ['less', 'more'] },
  { label: 'Steps that errored', get: (c) => c.errored_per_100_steps, fmt: (n) => `${n.toFixed(1)}%`, words: ['fewer', 'more'], points: true },
  { label: 'Steps in retries', get: (c) => c.retry_pct, fmt: (n) => `${n.toFixed(1)}%`, words: ['fewer', 'more'], points: true },
  { label: 'Calls stopped', get: (c) => c.stopped_per_100_calls, fmt: (n) => `${n.toFixed(1)}%`, words: ['fewer', 'more'], points: true },
  { label: 'Lines still in the code', get: (c) => c.lasted_pct, fmt: (n) => `${Math.round(n)}%`, words: ['less', 'more'], points: true },
]

function change(m: Metric, a: number | null, b: number | null): string {
  if (a == null || b == null) return '–'
  if (m.points) {
    const d = b - a
    if (Math.abs(d) < 0.5) return 'about the same'
    return `${Math.abs(d).toFixed(1)} points ${d < 0 ? m.words[0] : m.words[1]}`
  }
  if (a === 0) return b === 0 ? 'about the same' : '–'
  const r = (b - a) / a
  if (Math.abs(r) < 0.05) return 'about the same'
  return `${Math.round(Math.abs(r) * 100)}% ${r < 0 ? m.words[0] : m.words[1]}`
}

const label = (dim: CompareDim, value: string) =>
  dim === 'mode' ? (modeLabel(value) ?? value) : dim === 'date' ? (value === 'before' ? 'Before' : 'After') : value

/** Two groups of turns side by side. */
export function Compare() {
  const p = new URLSearchParams(useSearch())
  const [, navigate] = useLocation()
  const f = {
    range: p.get('range') ?? 'all',
    dim: (p.get('dim') ?? '') as CompareDim | '',
    a: p.get('a') ?? '',
    b: p.get('b') ?? '',
    at: p.get('at') ?? '',
    project: p.get('project') ?? '',
  }
  const set = (patch: Partial<typeof f>) => navigate(`/compare${query({ ...f, ...patch })}`, { replace: true })
  const gen = useIndexStatus().status?.generation ?? 0
  const opts = useApi<CompareOptions>(`/api/compare/options${query({ range: f.range })}`, gen)
  const ready = f.dim && f.a && f.b && (f.dim !== 'date' || f.at)
  const cmp = useApi<Comparison>(
    ready ? `/api/compare${query({ range: f.range, dim: f.dim, a: f.a, b: f.b, at: f.at, project: f.project })}` : null,
    gen,
  )
  const values = opts.data?.dims.find((d) => d.dim === f.dim)?.values ?? []

  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Compare</h1>
          <p>
            Two groups of turns side by side: before and after a model, agent version, effort, mode, skill or a change to
            AGENTS.md. Changes are described, not scored; the work in each group can differ too.
          </p>
        </div>
        <div className={v.controls}>
          <Segmented label="Time range" value={f.range} onChange={(range) => set({ range })} options={RANGES} />
        </div>
      </header>

      {opts.data && opts.data.suggestions.length > 0 && (
        <section aria-label="Suggested comparisons">
          <h2 className={s.h2}>
            <Lightbulb aria-hidden /> Found in your sessions
          </h2>
          <div className={s.chips}>
            {opts.data.suggestions.map((x) => {
              const on = f.dim === x.dim && f.a === x.a && f.b === x.b && f.at === (x.at ?? '') && f.project === (x.project ?? '')
              return (
                <button
                  key={x.title}
                  type="button"
                  className={`${s.chip} ${on ? s.chipOn : ''}`}
                  onClick={() => set({ dim: x.dim, a: x.a, b: x.b, at: x.at ?? '', project: x.project ?? '' })}
                >
                  <span>{x.title}</span>
                  <small>
                    {formatNumber(x.turns_a)} vs {formatNumber(x.turns_b)} turns
                  </small>
                </button>
              )
            })}
          </div>
        </section>
      )}

      <Panel title="Pick two groups" icon={<GitCompareArrows />}>
        <div className={s.pickers}>
          <Segmented
            label="Compare by"
            value={f.dim === 'date' ? '' : f.dim}
            onChange={(dim) => set({ dim: dim as CompareDim, a: '', b: '', at: '', project: '' })}
            options={DIMS}
          />
          {f.dim && f.dim !== 'date' && (
            <div className={s.selects}>
              {(['a', 'b'] as const).map((k) => (
                <label key={k} className={s.select}>
                  <span>{k === 'a' ? 'Group A' : 'Group B'}</span>
                  <select value={f[k]} onChange={(e) => set({ [k]: e.target.value })}>
                    <option value="">Choose…</option>
                    {values.map(([val, n]) => (
                      <option key={val} value={val}>
                        {label(f.dim as CompareDim, val)} ({n} turns)
                      </option>
                    ))}
                  </select>
                </label>
              ))}
            </div>
          )}
          {f.dim === 'date' && (
            <p className={s.note}>
              Before and after {formatDate(f.at)}
              {f.project ? ` in ${baseName(f.project)}` : ''}.
            </p>
          )}
        </div>
      </Panel>

      {!ready ? (
        <Panel>
          <EmptyState icon={<GitCompareArrows />} title="Pick a comparison">
            Choose one found in your sessions, or pick what to compare by and two values.
          </EmptyState>
        </Panel>
      ) : !cmp.data ? (
        <Skeleton height={320} />
      ) : (
        <Result c={cmp.data} />
      )}
    </div>
  )
}

function Result({ c }: { c: Comparison }) {
  const few = [c.a, c.b].filter((x) => x.turns < 20)
  const name = (x: Cohort) => label(c.dim, x.value)
  return (
    <Panel title={`${name(c.a)} vs ${name(c.b)}`} icon={<GitCompareArrows />} flush>
      <div className={s.cohorts}>
        {[c.a, c.b].map((x, i) => (
          <div key={i} className={s.cohort}>
            <span className={s.swatch} style={{ background: i ? 'var(--heat-4)' : 'var(--heat-2)' }} aria-hidden />
            <span className={s.cohortName}>
              {i ? 'B' : 'A'} · {name(x)}
            </span>
            <span className={s.cohortMeta}>
              {formatNumber(x.turns)} turns in {formatNumber(x.sessions)} sessions
              {x.first && x.last ? ` · ${formatDate(x.first)} – ${formatDate(x.last)}` : ''}
            </span>
          </div>
        ))}
      </div>
      {few.length > 0 && (
        <p className={s.warn}>
          <TriangleAlert aria-hidden style={{ color: 'var(--status-warning)' }} />
          Few turns in {few.map(name).join(' and ')}: small groups swing a lot, so read the differences with care.
        </p>
      )}
      <div className={v.tableWrap}>
        <table className={v.table}>
          <thead>
            <tr>
              <th>Measure</th>
              <th className={v.num}>A</th>
              <th className={v.num}>B</th>
              <th className={s.barCol} aria-hidden />
              <th>B compared with A</th>
            </tr>
          </thead>
          <tbody>
            {METRICS.map((m) => {
              const a = m.get(c.a)
              const b = m.get(c.b)
              const max = Math.max(a ?? 0, b ?? 0) || 1
              return (
                <tr key={m.label}>
                  <td>{m.label}</td>
                  <td className={v.num}>{a == null ? '–' : m.fmt(a)}</td>
                  <td className={v.num}>{b == null ? '–' : m.fmt(b)}</td>
                  <td className={s.barCol} aria-hidden>
                    <span className={s.bar} style={{ width: `${((a ?? 0) / max) * 100}%`, background: 'var(--heat-2)' }} />
                    <span className={s.bar} style={{ width: `${((b ?? 0) / max) * 100}%`, background: 'var(--heat-4)' }} />
                  </td>
                  <td className={s.change}>{change(m, a, b)}</td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
      <p className={s.foot}>
        Typical means the median. Rates are per 100 steps or tool calls, so groups of different sizes compare fairly. What
        lasted counts edits at least a day old.
      </p>
    </Panel>
  )
}
