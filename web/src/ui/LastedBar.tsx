import type { Lasted } from '../lib/api'
import { formatNumber } from '../lib/format'
import { PARTS, pct } from '../lib/lasted'
import { Tooltip } from './Tooltip'
import s from './LastedBar.module.css'

/** Added lines split into kept / rewritten by agents / changed since. */
export function LastedBar({ c, height = 8, label }: { c: Lasted; height?: number; label?: string }) {
  if (c.added === 0) return <div className={s.empty} style={{ height }} />
  return (
    <div
      className={s.bar}
      style={{ height }}
      role="img"
      aria-label={`${label ? label + ': ' : ''}${PARTS.map((p) => `${pct(c[p.key], c.added)} ${p.label.toLowerCase()}`).join(', ')}`}
    >
      {PARTS.map((p) =>
        c[p.key] > 0 ? (
          <span key={p.key} className={s.seg} style={{ flexGrow: c[p.key] }}>
            <Tooltip
              delay={80}
              content={
                <>
                  <b>{pct(c[p.key], c.added)}</b> {p.label.toLowerCase()} · {formatNumber(c[p.key])} of{' '}
                  {formatNumber(c.added)} lines
                </>
              }
            >
              <span className={s.part} style={{ background: p.color }} />
            </Tooltip>
          </span>
        ) : null,
      )}
    </div>
  )
}

/** The legend for every LastedBar on a page. */
export function LastedLegend() {
  return (
    <ul className={s.legend}>
      {PARTS.map((p) => (
        <li key={p.key} title={p.hint}>
          <span className={s.swatch} style={{ background: p.color }} aria-hidden />
          {p.label}
        </li>
      ))}
    </ul>
  )
}
