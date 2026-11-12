import { Check, Lightbulb } from 'lucide-react'
import { Link } from 'wouter'
import type { Fix } from '../lib/api'
import { formatCompact, formatCost, formatHours, formatNumber, oneLine } from '../lib/format'
import { Button } from './Button'
import { CodeText } from './Code'
import { CopyButton } from './CopyButton'
import { Sparkline } from './Sparkline'
import s from './FixCard.module.css'

/** "about 23m a week" or "about $36 a week". Always an estimate. */
function fixSize(f: Fix): string | null {
  if (f.ms_per_week != null) return `about ${formatHours(f.ms_per_week)} a week`
  if (f.usd_per_week != null) return `about ${formatCost(f.usd_per_week)} a week`
  if (f.tokens_per_week != null) return `about ${formatCompact(f.tokens_per_week)} tokens a week`
  return null
}

/**
 * One change worth making: what the logs show, how big it is, the steps
 * behind it, and something to copy. It never grades the agent or the person.
 */
export function FixCard({
  fix,
  onMark,
  onSeeAll,
  href,
}: {
  fix: Fix
  onMark?: (state: 'applied' | 'dismissed') => void
  /** Opens every occurrence, not only the few shown. */
  onSeeAll?: () => void
  /** Makes the title a link, for a card that sums up a page elsewhere. */
  href?: string
}) {
  const size = fixSize(fix)
  return (
    <article className={s.card}>
      <header className={s.head}>
        <Lightbulb aria-hidden className={s.icon} />
        <h3 className={s.title}>
          {href ? (
            <Link href={href} className={s.titleLink}>
              <CodeText text={fix.title} />
            </Link>
          ) : (
            <CodeText text={fix.title} />
          )}
        </h3>
        {size && (
          <span className={s.size} title="An estimate from your logs">
            {size}
          </span>
        )}
      </header>
      <p className={s.detail}>
        <CodeText text={fix.detail} />
      </p>
      {fix.advice && <p className={s.advice}>{fix.advice}</p>}
      {fix.fix && (
        <div className={s.code}>
          <div className={s.codeHead}>
            <span>{fix.fix_where}</span>
            <CopyButton text={fix.fix} />
          </div>
          <pre>{fix.fix}</pre>
        </div>
      )}
      <footer className={s.foot}>
        {fix.weekly.length > 1 && (
          <span className={s.weeks} title={`Times a week, oldest first: ${fix.weekly.join(', ')}`}>
            <Sparkline values={fix.weekly} width={Math.min(72, fix.weekly.length * 8)} height={16} />
            by week
          </span>
        )}
        {fix.examples.length > 0 && (
          <span className={s.examples}>
            See it:
            {fix.examples.map((e) => {
              const name = oneLine(e.title ?? 'Untitled session')
              return (
                <Link key={e.session_id + e.seq} href={`/sessions/${e.session_id}?step=${e.seq}`} title={name} className={s.example}>
                  {name.length > 28 ? name.slice(0, 27) + '…' : name}
                </Link>
              )
            })}
          </span>
        )}
        {onSeeAll && fix.count > fix.examples.length && (
          <button type="button" className={s.seeAll} onClick={onSeeAll}>
            See all {formatNumber(fix.count)}
          </button>
        )}
        {fix.prompt && <CopyButton text={fix.prompt} label="Copy as a prompt" className={s.prompt} />}
      </footer>
      {onMark && (
        <div className={s.marks}>
          <Button size="sm" onClick={() => onMark('applied')} title="agentcam will measure the weeks before against the time since">
            <Check aria-hidden /> I did this
          </Button>
          <Button size="sm" variant="ghost" onClick={() => onMark('dismissed')} title="Hide this fix; you can bring it back from the ledger">
            Not for me
          </Button>
        </div>
      )}
    </article>
  )
}
