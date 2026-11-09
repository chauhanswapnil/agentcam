import { useLayoutEffect, useRef } from 'react'
import s from './Segmented.module.css'

type Props<T extends string> = {
  options: { value: T; label: string }[]
  value: T
  onChange: (v: T) => void
  label: string
}

/**
 * A small set of mutually exclusive choices, like a time range. On a narrow
 * screen the group scrolls sideways and keeps the chosen option in view.
 */
export function Segmented<T extends string>({ options, value, onChange, label }: Props<T>) {
  const ref = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const g = ref.current
    const on = g?.querySelector<HTMLElement>('[aria-checked="true"]')
    if (!g || !on || g.scrollWidth <= g.clientWidth) return
    // Scroll the group only, never the page.
    const left = on.offsetLeft - g.offsetLeft
    if (left < g.scrollLeft || left + on.offsetWidth > g.scrollLeft + g.clientWidth) {
      g.scrollLeft = left + on.offsetWidth - g.clientWidth + 8
    }
  }, [value])
  return (
    <div ref={ref} className={s.group} role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          className={`${s.option} ${o.value === value ? s.active : ''}`}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}
