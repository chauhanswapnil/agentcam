import { CircleDollarSign, TriangleAlert } from 'lucide-react'
import type { Costs } from '../lib/api'
import { COST_PARTS as PARTS } from '../lib/cost'
import { formatCompact, formatCost, formatDate, formatNumber } from '../lib/format'
import { Panel } from './Panel'
import s from './CostPanel.module.css'

/** Where the estimated cost comes from: kinds of token, the cache, models. */
export function CostPanel({ data }: { data: Costs }) {
  const saved = data.without_cache_usd - data.usd
  const max = Math.max(...PARTS.map((p) => data.parts[p.key]), 0.000001)
  const tokens = (m: Costs['models'][number]) => m.input_tokens + m.output_tokens + m.cache_read_tokens + m.cache_write_tokens
  return (
    <Panel title="Cost" icon={<CircleDollarSign />}>
      <div className={s.top}>
        <div className={s.stat}>
          <span>Est. cost</span>
          <b>
            {formatCost(data.usd)}
            {data.unpriced_calls ? '+' : ''}
          </b>
        </div>
        {data.usd > 0 && (
          <div className={s.stat} title="The same calls with every prompt token at the fresh-input price">
            <span>Without the cache</span>
            <b>{formatCost(data.without_cache_usd)}</b>
          </div>
        )}
        {saved >= 0.01 && (
          <div className={s.stat} title="Cache reads cost a fraction of fresh input; cache writes cost a little more">
            <span>The cache saved</span>
            <b>{formatCost(saved)}</b>
          </div>
        )}
        {data.cache_hit_rate != null && (
          <div className={s.stat} title="Share of prompt tokens read from the cache">
            <span>Read from cache</span>
            <b>{Math.round(data.cache_hit_rate * 100)}%</b>
          </div>
        )}
      </div>

      {data.usd > 0 && (
        <ul className={s.parts} aria-label="Estimated cost by kind of token">
          {PARTS.map((p) => {
            const v = data.parts[p.key]
            return (
              <li key={p.key} title={p.hint}>
                <span className={s.partLabel}>{p.label}</span>
                <span className={s.track}>
                  <span className={s.bar} style={{ width: `${(v / max) * 100}%` }} />
                </span>
                <span className={s.partNum}>{formatCost(v)}</span>
                <span className={s.partShare}>{Math.round((v / data.usd) * 100)}%</span>
              </li>
            )
          })}
        </ul>
      )}

      {data.models.length > 0 && (
        <table className={s.table}>
          <thead>
            <tr>
              <th>Model</th>
              <th className={s.num}>Calls</th>
              <th className={s.num}>Tokens</th>
              <th className={s.num}>Est. cost</th>
            </tr>
          </thead>
          <tbody>
            {data.models.slice(0, 8).map((m) => (
              <tr key={m.model ?? '-'}>
                <td className={m.model == null ? s.muted : s.model}>{m.model ?? 'Not recorded'}</td>
                <td className={s.num}>{formatNumber(m.calls)}</td>
                <td className={s.num}>{formatCompact(tokens(m))}</td>
                <td className={`${s.num} ${m.usd == null ? s.muted : ''}`}>{m.usd == null ? 'no price' : formatCost(m.usd)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <p className={s.note}>
        At API list prices, checked {formatDate(data.priced_on + 'T12:00:00')}. On a subscription you pay the plan price, not this.
        {data.fast_calls > 0 && <> {formatNumber(data.fast_calls)} calls ran fast and are priced at the fast rate.</>}
      </p>
      {data.unpriced_calls > 0 && (
        <p className={s.note}>
          <TriangleAlert aria-hidden className={s.warn} />
          {formatNumber(data.unpriced_calls)} calls on {data.unpriced_models.join(', ')} have no price and are left out. Add one to <code>prices.yaml</code> next to
          the index.
        </p>
      )}
      {data.prices_error && (
        <p className={s.note}>
          <TriangleAlert aria-hidden className={s.warn} />
          Your <code>prices.yaml</code> couldn't be read, so the built-in prices are in use. {data.prices_error}
        </p>
      )}
    </Panel>
  )
}
