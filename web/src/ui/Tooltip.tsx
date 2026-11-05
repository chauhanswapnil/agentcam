import { useId, useRef, useState, type ReactNode } from 'react'
import s from './Tooltip.module.css'

type Props = {
  content: ReactNode
  children: ReactNode
  side?: 'top' | 'bottom'
  /** Hover delay before showing. Focus shows it at once. */
  delay?: number
}

/** Small label on hover or keyboard focus. Not for anything interactive. */
export function Tooltip({ content, children, side = 'top', delay = 350 }: Props) {
  const id = useId()
  const [open, setOpen] = useState(false)
  const timer = useRef<number | undefined>(undefined)
  const show = (wait: number) => {
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => setOpen(true), wait)
  }
  const hide = () => {
    window.clearTimeout(timer.current)
    setOpen(false)
  }
  return (
    <span
      className={s.wrap}
      onMouseEnter={() => show(delay)}
      onMouseLeave={hide}
      onFocus={() => show(0)}
      onBlur={hide}
      aria-describedby={open ? id : undefined}
    >
      {children}
      {/* Only in the page while shown: a hidden tooltip near the edge would
          still widen the page and cause sideways scrolling. */}
      {open && (
        <span role="tooltip" id={id} className={`${s.tip} ${side === 'bottom' ? s.bottom : ''}`}>
          {content}
        </span>
      )}
    </span>
  )
}
