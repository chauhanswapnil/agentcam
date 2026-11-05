import type { ButtonHTMLAttributes } from 'react'
import s from './Button.module.css'

type Props = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'default' | 'ghost' | 'primary'
  size?: 'sm' | 'md'
  /** Square button holding only an icon. Give it an aria-label. */
  icon?: boolean
}

export function Button({ variant = 'default', size = 'md', icon, className, ...rest }: Props) {
  const cls = [
    s.button,
    variant !== 'default' && s[variant],
    size === 'sm' && s.sm,
    icon && s.icon,
    className,
  ]
    .filter(Boolean)
    .join(' ')
  return <button type="button" className={cls} {...rest} />
}
