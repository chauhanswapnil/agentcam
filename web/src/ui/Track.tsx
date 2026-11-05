import { memo, useCallback, useEffect, useRef, useState } from 'react'
import { formatDuration, formatTime } from '../lib/format'
import { GROUPS, groupOf, kindLabel } from '../lib/kinds'
import type { EventRow, Severity } from '../lib/api'
import { SEVERITY_COLOR } from '../lib/severity'
import { KindIcon } from './Misc'
import s from './Track.module.css'

type Props = {
  events: EventRow[]
  current: number
  onSeek: (i: number) => void
  /** 1 fits the whole session; higher values stretch it and scroll. */
  zoom: number
  /** Worst risk finding per step seq, drawn as a flag above the mark. */
  flags?: Map<number, Severity>
}

const weight = (e: EventRow) => 1 + Math.log10(1 + (e.duration_ms ?? 0) / 250)

/**
 * The replay timeline: one mark per step, wider for steps that took longer.
 * Steps before the playhead are brighter than steps after it.
 */
export function Track({ events, current, onSeek, zoom, flags }: Props) {
  const outer = useRef<HTMLDivElement>(null)
  const viewport = useRef<HTMLDivElement>(null)
  const [hover, setHover] = useState<{ i: number; x: number } | null>(null)

  // Keep the playhead in view when zoomed in.
  useEffect(() => {
    const el = viewport.current?.querySelector<HTMLElement>(`[data-i="${current}"]`)
    el?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }, [current, zoom])

  // Stable handlers so memoized marks don't re-render on every hover.
  const show = useCallback((i: number, el: HTMLElement) => {
    const box = outer.current!.getBoundingClientRect()
    const r = el.getBoundingClientRect()
    setHover({ i, x: Math.min(Math.max(r.left - box.left + r.width / 2, 80), box.width - 80) })
  }, [])
  const hide = useCallback(() => setHover(null), [])
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
            <Mark
              key={e.seq}
              e={e}
              i={i}
              state={i === current ? 'on' : i < current ? 'past' : 'future'}
              onSeek={onSeek}
              onHover={show}
              onLeave={hide}
              flag={flags?.get(e.seq)}
            />
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

/** One step on the track. Memoized: playback re-renders only marks whose
 * state changed. */
const Mark = memo(function Mark({
  e,
  i,
  state,
  onSeek,
  onHover,
  onLeave,
  flag,
}: {
  e: EventRow
  i: number
  state: 'past' | 'on' | 'future'
  onSeek: (i: number) => void
  onHover: (i: number, el: HTMLElement) => void
  onLeave: () => void
  flag?: Severity
}) {
  return (
    <button
      type="button"
      data-i={i}
      role="option"
      aria-selected={state === 'on'}
      aria-label={`Step ${i + 1}, ${kindLabel(e.kind, e.category)}: ${e.title}`}
      tabIndex={state === 'on' ? 0 : -1}
      className={`${s.mark} ${state === 'past' ? s.past : ''} ${state === 'on' ? s.current : ''}`}
      style={{ flexGrow: weight(e), background: GROUPS[groupOf(e.kind, e.category, e.is_error)].color }}
      onMouseEnter={(ev) => onHover(i, ev.currentTarget)}
      onMouseLeave={onLeave}
      onClick={() => onSeek(i)}
    >
      {state === 'on' && <span className={s.playhead} aria-hidden />}
      {flag && <span className={s.flag} style={{ background: SEVERITY_COLOR[flag] }} aria-label={`${flag} risk`} />}
    </button>
  )
})
