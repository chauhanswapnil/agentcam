import type { ReactNode } from 'react'
import s from './Stat.module.css'

/** One headline number with a label above and context below. */
export function Stat({ label, value, sub }: { label: string; value: ReactNode; sub?: ReactNode }) {
  return (
    <div className={s.stat}>
      <span className={s.label}>{label}</span>
      <span className={s.value}>{value}</span>
      {sub && <span className={s.sub}>{sub}</span>}
    </div>
  )
}
