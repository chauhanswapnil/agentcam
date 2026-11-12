import type { ReactNode } from 'react'
import s from './ListRow.module.css'

type Props = {
  leading?: ReactNode
  title: ReactNode
  meta?: ReactNode
  /** Shown on the right. Links and buttons in here work on their own; the rest of the row opens it. */
  trailing?: ReactNode
  selected?: boolean
  onClick?: () => void
}

/**
 * One clickable row in a list: icon, title over meta line, right-side details.
 * The title is the button and stretches over the whole row, so the trailing
 * details can hold links of their own without a button inside a button.
 */
export function ListRow({ leading, title, meta, trailing, selected, onClick }: Props) {
  return (
    <div className={`${s.row} ${selected ? s.selected : ''}`}>
      <span className={s.leading}>{leading}</span>
      <span className={s.main}>
        <button type="button" className={s.title} aria-current={selected || undefined} onClick={onClick}>
          {title}
        </button>
        {meta && <span className={s.meta}>{meta}</span>}
      </span>
      {trailing && <span className={s.trailing}>{trailing}</span>}
    </div>
  )
}
