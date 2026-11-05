import type { ReactNode } from 'react'
import s from './ListRow.module.css'

type Props = {
  leading?: ReactNode
  title: ReactNode
  meta?: ReactNode
  trailing?: ReactNode
  selected?: boolean
  onClick?: () => void
}

/** One clickable row in a list: icon, title over meta line, right-side details. */
export function ListRow({ leading, title, meta, trailing, selected, onClick }: Props) {
  return (
    <button
      type="button"
      className={`${s.row} ${selected ? s.selected : ''}`}
      aria-current={selected || undefined}
      onClick={onClick}
    >
      <span className={s.leading}>{leading}</span>
      <span className={s.main}>
        <span className={s.title}>{title}</span>
        {meta && <span className={s.meta}>{meta}</span>}
      </span>
      {trailing && <span className={s.trailing}>{trailing}</span>}
    </button>
  )
}
