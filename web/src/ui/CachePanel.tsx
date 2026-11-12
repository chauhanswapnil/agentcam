import { DatabaseZap } from 'lucide-react'
import { Link } from 'wouter'
import type { Cache } from '../lib/api'
import { CAUSES, describeRebuild } from '../lib/cache'
import { formatAgo, formatCompact, formatCost, formatNumber, oneLine } from '../lib/format'
import { Panel } from './Panel'
import s from './CachePanel.module.css'

/** How much of each prompt came from the cache, and where it was written again. */
export function CachePanel({ data }: { data: Cache }) {
  return (
    <Panel title="Prompt cache" icon={<DatabaseZap />}>
      <div className={s.top}>
        {data.hit_rate != null && (
          <div className={s.stat} title="Share of prompt tokens read from the cache, at a fraction of the input price">
            <span>Read from cache</span>
            <b>{Math.round(data.hit_rate * 100)}%</b>
          </div>
        )}
        <div className={s.stat} title="Calls that wrote most of the prompt to the cache again, after it had been reading from it">
          <span>Rebuilds</span>
          <b>{formatNumber(data.rebuilds)}</b>
        </div>
        {data.rebuilds > 0 && (
          <div className={s.stat} title="What writing those tokens cost over reading them, at API list prices">
            <span>They cost about</span>
            <b>{formatCost(data.rebuild_usd)}</b>
          </div>
        )}
      </div>

      {data.rebuilds === 0 ? (
        <p className={s.note}>The cache held all through this range: no call had to write the prompt again.</p>
      ) : (
        <>
          <ul className={s.causes}>
            {data.by_cause.map((c) => (
              <li key={c.cause}>
                <div className={s.causeHead}>
                  <span>{CAUSES[c.cause].label}</span>
                  <span className={s.num}>
                    {formatNumber(c.count)} · {formatCompact(c.tokens)} tokens · {formatCost(c.usd)}
                  </span>
                </div>
                <p>{CAUSES[c.cause].next}</p>
              </li>
            ))}
          </ul>
          <div className={s.head}>Latest</div>
          <ul className={s.latest}>
            {data.latest.slice(0, 5).map((b) => (
              <li key={b.session_id + b.seq}>
                <Link href={`/sessions/${b.session_id}?step=${b.seq}`} className={s.row}>
                  <span className={s.text}>{oneLine(b.title ?? 'Untitled session')}</span>
                  <span className={s.sub}>
                    {describeRebuild(b)}
                    {b.ts ? ` · ${formatAgo(b.ts)}` : ''}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}
      <p className={s.note}>
        A rebuild after a break is how the cache works, not a fault. Causes are read from the log, which doesn't hold the prompt itself.
        {data.compactions > 0 && <> {formatNumber(data.compactions)} compactions rewrote the prompt on purpose and aren't counted.</>}
      </p>
    </Panel>
  )
}
