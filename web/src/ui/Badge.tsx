import type { ReactNode } from 'react'
import s from './Badge.module.css'

type Props = {
  children: ReactNode
  /** Color of the leading dot or icon. The text stays in text colors. */
  color?: string
  /** Show a dot in `color` instead of an icon. */
  dot?: boolean
  icon?: ReactNode
  solid?: boolean
  title?: string
}

export function Badge({ children, color, dot, icon, solid, title }: Props) {
  return (
    <span className={`${s.badge} ${solid ? s.solid : ''}`} title={title}>
      {dot && <span className={s.dot} style={{ background: color ?? 'var(--text-3)' }} />}
      {icon && <span style={{ color, display: 'inline-flex' }}>{icon}</span>}
      {children}
    </span>
  )
}
