import type { ReactNode } from 'react'
import s from './Panel.module.css'

type Props = {
  title?: ReactNode
  icon?: ReactNode
  actions?: ReactNode
  /** No padding around the body, for lists and charts that run edge to edge. */
  flush?: boolean
  children: ReactNode
  className?: string
}

export function Panel({ title, icon, actions, flush, children, className }: Props) {
  return (
    <section className={`${s.panel} ${className ?? ''}`}>
      {(title || actions) && (
        <header className={s.header}>
          <h3 className={s.title}>
            {icon}
            {title}
          </h3>
          {actions && <div className={s.actions}>{actions}</div>}
        </header>
      )}
      <div className={`${s.body} ${flush ? s.flush : ''}`}>{children}</div>
    </section>
  )
}
