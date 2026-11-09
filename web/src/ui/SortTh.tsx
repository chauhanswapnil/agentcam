import { ArrowDown, ArrowUp } from 'lucide-react'
import s from './SortTh.module.css'

/**
 * A table header that sorts by its column. Click once for largest first,
 * again for smallest first.
 */
export function SortTh({
  id,
  label,
  sort,
  onSort,
  num,
  title,
}: {
  id: string
  label: string
  /** Current sort: a column id, `-` in front for ascending. */
  sort: string
  onSort: (next: string) => void
  num?: boolean
  /** Longer wording, shown on hover. */
  title?: string
}) {
  const on = sort === id || sort === `-${id}`
  const asc = sort === `-${id}`
  return (
    <th className={num ? s.num : undefined} title={title} aria-sort={on ? (asc ? 'ascending' : 'descending') : 'none'}>
      <button type="button" className={`${s.btn} ${on ? s.on : ''}`} onClick={() => onSort(on && !asc ? `-${id}` : id)}>
        {label}
        {on && (asc ? <ArrowUp aria-hidden /> : <ArrowDown aria-hidden />)}
      </button>
    </th>
  )
}
