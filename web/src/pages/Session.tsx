import { ArrowLeft, Bot, CalendarClock, FolderGit2, GitBranch, Play, Rows3, Sparkles } from 'lucide-react'
import { useState } from 'react'
import { Link, useLocation, useParams } from 'wouter'
import { useApi, type SessionDetail } from '../lib/api'
import { baseName, formatCompact, formatDate, formatDuration, formatHours, formatNumber, formatTime, oneLine } from '../lib/format'
import { EventStrip } from '../ui/EventStrip'
import { ListRow } from '../ui/ListRow'
import { AgentLabel, EmptyState, KindIcon, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Stat } from '../ui/Stat'
import s from './Session.module.css'

/** One session: header, facts and the step strip. The full replay builds on this. */
export function Session() {
  const params = useParams()
  const id = params['*'] ?? ''
  const [, navigate] = useLocation()
  const { data, error } = useApi<SessionDetail>(`/api/sessions/${id}`)
  const [step, setStep] = useState<number | undefined>(undefined)

  if (error) {
    return (
      <div className={s.page}>
        <Link href="/" className={s.back}>
          <ArrowLeft aria-hidden /> Overview
        </Link>
        <EmptyState title={error === 'Not found' ? 'Session not found' : 'Could not load this session'}>{error}</EmptyState>
      </div>
    )
  }
  if (!data) {
    return (
      <div className={s.page}>
        <Skeleton width="40%" height={24} />
        <Skeleton height={80} />
      </div>
    )
  }

  const { session: x, events, subagents } = data
  const current = step != null ? events[step] : undefined
  const title = oneLine(x.title ?? x.first_prompt ?? 'Untitled session')

  return (
    <div className={s.page}>
      <Link href={x.parent_id ? `/sessions/${x.parent_id}` : '/'} className={s.back}>
        <ArrowLeft aria-hidden /> {x.parent_id ? 'Parent session' : 'Overview'}
      </Link>

      <header className={s.head}>
        <h1>{title}</h1>
        <div className={s.meta}>
          <AgentLabel agent={x.agent} />
          {x.project_path && (
            <span title={x.project_path}>
              <FolderGit2 aria-hidden /> {baseName(x.project_root ?? x.project_path)}
            </span>
          )}
          {x.git_branch && (
            <span className={s.mono}>
              <GitBranch aria-hidden /> {x.git_branch}
            </span>
          )}
          {x.model && (
            <span className={s.mono}>
              <Sparkles aria-hidden /> {x.model}
            </span>
          )}
          {x.started_at && (
            <span>
              <CalendarClock aria-hidden /> {formatDate(x.started_at)}, {formatTime(x.started_at)}
            </span>
          )}
        </div>
      </header>

      <Panel>
        <div className={s.facts}>
          <Stat label="Agent time" value={formatHours(x.active_ms)} />
          <Stat label="Steps" value={formatNumber(x.event_count)} sub={`${formatNumber(x.tool_call_count)} tool calls`} />
          <Stat label="Files changed" value={formatNumber(x.file_count)} />
          <Stat label="Failed steps" value={formatNumber(x.error_count)} />
          <Stat
            label="Tokens"
            value={formatCompact(x.input_tokens + x.cache_read_tokens + x.output_tokens)}
            sub={`${formatCompact(x.output_tokens)} out${x.cost_usd != null ? ` · $${x.cost_usd.toFixed(2)}` : ''}`}
          />
        </div>
      </Panel>

      <Panel title="Timeline" icon={<Play />}>
        {events.length ? (
          <>
            <EventStrip
              label="Session steps"
              current={step}
              onSelect={setStep}
              events={events.map((e) => ({
                kind: e.kind,
                category: e.category,
                isError: e.is_error,
                durationMs: e.duration_ms,
                title: e.title,
              }))}
            />
            <div className={s.detail}>
              {current ? (
                <>
                  <KindIcon kind={current.kind} category={current.category} isError={current.is_error} />
                  <span>
                    Step {step! + 1} of {events.length}
                  </span>
                  <span className={s.detailTitle}>{current.title}</span>
                  {current.duration_ms != null && <span className="num">{formatDuration(current.duration_ms)}</span>}
                </>
              ) : (
                <span className={s.note}>Click a step to see what it was. The full replay, with prompts, output and diffs, comes next.</span>
              )}
            </div>
          </>
        ) : (
          <EmptyState title="No steps recorded" />
        )}
      </Panel>

      {subagents.length > 0 && (
        <Panel title={`Subagents · ${subagents.length}`} icon={<Bot />} flush>
          {subagents.map((sub) => (
            <ListRow
              key={sub.id}
              onClick={() => navigate(`/sessions/${sub.id}`)}
              leading={<Rows3 aria-hidden style={{ width: 14, color: 'var(--text-3)' }} />}
              title={oneLine(sub.title ?? sub.first_prompt ?? sub.id)}
              meta={`${formatNumber(sub.event_count)} steps`}
              trailing={formatHours(sub.active_ms)}
            />
          ))}
        </Panel>
      )}
    </div>
  )
}
