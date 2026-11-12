import { Link } from 'wouter'
import type { Behind } from '../lib/api'
import { query, useApi } from '../lib/api'
import { formatBehind } from '../lib/behind'
import { formatAgo, formatNumber } from '../lib/format'
import { Drawer } from './Drawer'
import { EmptyState, Skeleton } from './Misc'
import s from './StepsDrawer.module.css'

type Props = {
  /** The number to list steps for: `time:tools`, `cost:output`, `fix:<id>`… */
  what: string
  filter: { range?: string; agent?: string; project?: string; day?: string; since?: string }
  /** Narrow to one session and its subagents. */
  session?: string
  onClose: () => void
}

/** The rows alone: each step a link to that step in the replay. */
export function StepsList({ data }: { data: Behind }) {
  if (data.items.length === 0) return <EmptyState title="Nothing behind this in the range" />
  return (
    <>
      {data.total > data.items.length && (
        <p className={s.note}>
          Showing the top {data.items.length} of {formatNumber(data.total)}
        </p>
      )}
      <ul className={s.list}>
        {data.items.map((x) => (
          <li key={`${x.session_id}:${x.seq}`}>
            <Link href={`/sessions/${x.session_id}?step=${x.seq}`} className={s.row}>
              <span className={s.what}>
                <span className={s.step}>{x.step_title || `Step ${x.seq}`}</span>
                <span className={s.where}>{[x.session_title, x.ts ? formatAgo(x.ts) : null].filter(Boolean).join(' · ')}</span>
              </span>
              <span className={s.value}>{formatBehind(x.value, data.unit)}</span>
            </Link>
          </li>
        ))}
      </ul>
    </>
  )
}

/** The steps behind a number, in a drawer. */
export function StepsDrawer({ what, filter, session, onClose }: Props) {
  const found = useApi<Behind>(`/api/behind${query({ what, session, ...filter })}`)
  const data = found.data
  return (
    <Drawer title={data?.title ?? 'Steps'} label={data ? `Steps behind: ${data.title}` : 'Steps behind this number'} onClose={onClose}>
      {found.error ? (
        <p className={s.error}>Couldn't load these steps: {found.error}</p>
      ) : !data ? (
        <div className={s.skeletons}>
          {Array.from({ length: 6 }, (_, i) => (
            <Skeleton key={i} height={32} />
          ))}
        </div>
      ) : (
        <StepsList data={data} />
      )}
    </Drawer>
  )
}
