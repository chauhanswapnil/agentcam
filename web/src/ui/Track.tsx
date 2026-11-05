import { useEffect, useRef, useState } from 'react'
import { formatDuration, formatTime } from '../lib/format'
import { GROUPS, groupOf, kindLabel } from '../lib/kinds'
import type { EventRow } from '../lib/api'
import { KindIcon } from './Misc'
import s from './Track.module.css'

type Props = {
  events: EventRow[]
  current: number
  onSeek: (i: number) => void
  /** 1 fits the whole session; higher values stretch it and scroll. */
  zoom: number
}

const weight = (e: EventRow) => 1 + Math.log10(1 + (e.duration_ms ?? 0) / 250)

/**
 * The replay timeline: one mark per step, wider for steps that took longer.
 * Steps before the playhead are brighter than steps after it.
 */
export function Track({ events, current, onSeek, zoom }: Props) {
  const outer = useRef<HTMLDivElement>(null)
  const viewport = useRef<HTMLDivElement>(null)
  const [hover, setHover] = useState<{ i: number; x: number } | null>(null)

  // Keep the playhead in view when zoomed in.
  useEffect(() => {
    const el = viewport.current?.querySelector<HTMLElement>(`[data-i="${current}"]`)
    el?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }, [current, zoom])

  const show = (i: number, el: HTMLElement) => {
    const box = outer.current!.getBoundingClientRect()
    const r = el.getBoundingClientRect()
    setHover({ i, x: Math.min(Math.max(r.left - box.left + r.width / 2, 80), box.width - 80) })
  }
  const first = events.find((e) => e.ts)?.ts
  const last = [...events].reverse().find((e) => e.ts)?.ts
  const h = hover && events[hover.i]

  return (
    <div className={s.outer} ref={outer}>
      <div className={s.viewport} ref={viewport}>
        <div
          className={s.lane}
          role="listbox"
          aria-label="Session steps"
          style={{ width: `${zoom * 100}%`, minWidth: events.length * 3 }}
        >
          {events.map((e, i) => (
            <button
              key={e.seq}
              type="button"
              data-i={i}
              role="option"
              aria-selected={i === current}
              aria-label={`Step ${i + 1}, ${kindLabel(e.kind, e.category)}: ${e.title}`}
              tabIndex={i === current ? 0 : -1}
              className={`${s.mark} ${i < current ? s.past : ''} ${i === current ? s.current : ''}`}
              style={{ flexGrow: weight(e), background: GROUPS[groupOf(e.kind, e.category, e.is_error)].color }}
              onMouseEnter={(ev) => show(i, ev.currentTarget)}
              onMouseLeave={() => setHover(null)}
              onClick={() => onSeek(i)}
            >
              {i === current && <span className={s.playhead} aria-hidden />}
            </button>
          ))}
        </div>
      </div>
      <div className={s.ends}>
        <span>{first ? formatTime(first) : ''}</span>
        <span>{last ? formatTime(last) : ''}</span>
      </div>
      {h && hover && (
        <div className={s.tip} style={{ left: hover.x }} role="tooltip">
          <KindIcon kind={h.kind} category={h.category} isError={h.is_error} />
          <span className={s.tipTitle}>{h.title}</span>
          {h.duration_ms != null && <span className={s.tipMeta}>{formatDuration(h.duration_ms)}</span>}
        </div>
      )}
    </div>
  )
}
