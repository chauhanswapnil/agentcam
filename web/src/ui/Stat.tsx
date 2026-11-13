import type { ReactNode } from 'react'
import { Link } from 'wouter'
import s from './Stat.module.css'

/**
 * One headline number with a label above and context below. With `href` or
 * `onClick` the label and number open what they sum up; `sub` may hold its
 * own link. Without either, nothing here is clickable.
 */
export function Stat({
  label,
  value,
  sub,
  href,
  onClick,
}: {
  label: string
  value: ReactNode
  sub?: ReactNode
  href?: string
  onClick?: () => void
}) {
  const head = (
    <>
      <span className={s.label}>{label}</span>
      <span className={s.value}>{value}</span>
    </>
  )
  return (
    <div className={s.stat}>
      {href ? (
        <Link href={href} className={`${s.head} ${s.open}`}>
          {head}
        </Link>
      ) : onClick ? (
        <button type="button" className={`${s.head} ${s.open}`} onClick={onClick}>
          {head}
        </button>
      ) : (
        <span className={s.head}>{head}</span>
      )}
      {sub && <span className={s.sub}>{sub}</span>}
    </div>
  )
}
