import { useRef, useState } from 'react'
import { formatDuration } from '../lib/format'
import { GROUPS, groupOf, kindLabel, type EventKind, type ToolCategory } from '../lib/kinds'
import { KindIcon } from './Misc'
import s from './EventStrip.module.css'

export type StripEvent = {
  kind: EventKind
  category?: ToolCategory | null
  isError?: boolean
  durationMs?: number | null
  title: string
}

type Props = {
  events: StripEvent[]
  /** Index of the step under the playhead. */
  current?: number
  onSelect?: (i: number) => void
  label: string
}

/** Width grows with duration on a log scale, so long steps stand out
 * without crushing the short ones. */
function weight(e: StripEvent) {
  return 1 + Math.log10(1 + (e.durationMs ?? 0) / 250)
}

/**
 * A session as a row of marks, one per step, colored by group. The first
 * draft of the replay timeline track.
 */
export function EventStrip({ events, current, onSelect, label }: Props) {
  const ref = useRef<HTMLDivElement>(null)
  const [hover, setHover] = useState<{ i: number; x: number } | null>(null)
  const centerOf = (el: HTMLElement) => {
    const box = ref.current!.getBoundingClientRect()
    const r = el.getBoundingClientRect()
    return r.left - box.left + r.width / 2
  }

  return (
    <div className={`${s.wrap} ${current != null ? s.dimmed : ''}`} ref={ref}>
      <div className={s.strip} role="listbox" aria-label={label}>
        {events.map((e, i) => {
          const g = GROUPS[groupOf(e.kind, e.category, e.isError)]
          return (
            <button
              key={i}
              type="button"
              role="option"
              aria-selected={i === current}
              aria-label={`${i + 1}. ${kindLabel(e.kind, e.category)}: ${e.title}`}
              className={`${s.mark} ${i === current ? s.current : ''}`}
              style={{ flexGrow: weight(e), background: g.color }}
              onMouseEnter={(ev) => setHover({ i, x: centerOf(ev.currentTarget) })}
              onMouseLeave={() => setHover(null)}
              onFocus={(ev) => setHover({ i, x: centerOf(ev.currentTarget) })}
              onBlur={() => setHover(null)}
              onClick={() => onSelect?.(i)}
            >
              {i === current && <span className={s.playhead} aria-hidden />}
            </button>
          )
        })}
      </div>
      {hover && (
        <div className={s.tip} style={{ left: hover.x }} role="tooltip">
          <KindIcon kind={events[hover.i].kind} category={events[hover.i].category} isError={events[hover.i].isError} />
          <span className={s.tipTitle}>{events[hover.i].title}</span>
          {events[hover.i].durationMs != null && (
            <span className={s.tipMeta}>{formatDuration(events[hover.i].durationMs!)}</span>
          )}
        </div>
      )}
    </div>
  )
}
