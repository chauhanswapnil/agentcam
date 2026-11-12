import {
  ArrowLeft,
  Ban,
  ChartNoAxesGantt,
  Gauge,
  GitCommitHorizontal,
  RotateCcw,
  CalendarClock,
  ChevronLeft,
  ChevronRight,
  FileStack,
  FolderGit2,
  GitBranch,
  Keyboard,
  ScrollText,
  CircleCheck,
  CircleMinus,
  CornerLeftUp,
  CornerRightDown,
  Link2,
  MessagesSquare,
  GitPullRequest,
  Puzzle,
  MonitorSmartphone,
  Pause,
  Play,
  Rows3,
  ShieldAlert,
  SkipForward,
  Sparkles,
  TriangleAlert,
  ZoomIn,
  ZoomOut,
} from 'lucide-react'
import { Fragment, createElement, memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Link, useLocation, useParams } from 'wouter'
import { query, getJson, useApi, useIndexStatus, type Behind, type BehindStep, type Cache, type Costs, type EventRow, type TimeBar as TimeBarData, type Lasted, type TurnRow, type FindingRow, type SessionDetail, type Severity } from '../lib/api'
import { TimeStrip } from '../ui/TimeBar'
import { Drawer } from '../ui/Drawer'
import { CostPanel } from '../ui/CostPanel'
import { describeRebuild } from '../lib/cache'
import { costSummary } from '../lib/cost'
import { baseName, formatCompact, formatCost, formatDate, formatDuration, formatHours, formatNumber, formatTime, oneLine } from '../lib/format'
import { GROUPS, groupOf, kindLabel } from '../lib/kinds'
import { entrypointLabel } from '../lib/live'
import { approvalInfo, modeLabel, viaLabel } from '../lib/turns'
import { ruleText } from '../lib/follow'
import { CodeText } from '../ui/Code'
import { Badge } from '../ui/Badge'
import { Button } from '../ui/Button'
import { CopyButton } from '../ui/CopyButton'
import { FindingReview } from '../ui/FindingReview'
import { DiffView } from '../ui/DiffView'
import { Kbd } from '../ui/Kbd'
import { Markdown } from '../ui/Markdown'
import { ReportCardButton } from '../ui/ReportCard'
import { EditorPicker, ExportLinks, OpenInEditor, ResumeButton } from '../ui/SessionActions'
import { firstLine } from '../lib/session'
import { BlockDialog } from '../ui/BlockDialog'
import { sessionCard } from '../lib/reportCard'
import { PlanPanel } from '../ui/PlanPanel'
import { SubagentTree } from '../ui/SubagentTree'
import { LastedBar } from '../ui/LastedBar'
import { AgentLabel, EmptyState, KindIcon, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import { SEVERITY_COLOR, SEVERITY_RANK } from '../lib/severity'
import { SeverityBadge } from '../ui/Severity'
import { Tooltip } from '../ui/Tooltip'
import { Track } from '../ui/Track'
import { TraceView } from '../ui/TraceView'
import { ContextChart } from '../ui/ContextChart'
import s from './Replay.module.css'

const SPEEDS = [
  { value: '1', label: '1×' },
  { value: '4', label: '4×' },
  { value: '16', label: '16×' },
]
const ZOOMS = [1, 2, 4, 8, 16]


export function Replay() {
  const id = useParams()['*'] ?? ''
  const { status } = useIndexStatus()
  const [openedAt] = useState(() => Date.now())
  const [ended, setEnded] = useState<string | null>(null)
  // A session that was active in the last 15 minutes may still be running:
  // refetch it after each index run so new steps appear.
  const live = !!ended && openedAt - Date.parse(ended) < 15 * 60_000
  // Bumped after marking a finding, to refetch its review state.
  const [rev, setRev] = useState(0)
  const { data, error } = useApi<SessionDetail>(`/api/sessions/${id}`, (live ? (status?.generation ?? 0) : 0) * 1000 + rev)
  if (data && data.session.ended_at !== ended) setEnded(data.session.ended_at)

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
        <Skeleton height={90} />
        <Skeleton height={300} />
      </div>
    )
  }
  return <Player key={id} id={id} data={data} onReviewed={() => setRev((n) => n + 1)} />
}

function Player({ id, data, onReviewed }: { id: string; data: SessionDetail; onReviewed: () => void }) {
  // Refetched when the session gains steps, so a live session's split keeps up.
  const cost = useApi<Costs>(`/api/cost${query({ session: id })}`, data.events.length)
  const cache = useApi<Cache>(`/api/cache${query({ session: id })}`, data.events.length)
  const time = useApi<TimeBarData>(`/api/profile${query({ session: id })}`, data.events.length)
  const { session: x, events, files, findings, patterns, commits, subagents, lasted, turns, prs, follow, plan } = data
  const [view, setView] = useState<'transcript' | 'turns' | 'trace'>('transcript')
  const [, navigate] = useLocation()
  // Read once: the URL follows the playhead, not the other way round.
  const [search] = useState(() => window.location.search)
  const last = events.length - 1
  const fromUrl = Number(new URLSearchParams(search).get('step'))
  const [step, setStep] = useState(() => (Number.isFinite(fromUrl) ? Math.min(Math.max(fromUrl, 0), Math.max(last, 0)) : 0))
  const [playing, setPlaying] = useState(false)
  const [speed, setSpeed] = useState('4')
  const [zoom, setZoom] = useState(events.length > 400 ? 4 : 1)
  const stepRef = useRef(step)
  // Steps that created, edited or deleted a file.
  const changed = useMemo(() => new Set(files.filter((f) => f.action !== 'read').map((f) => f.seq)), [files])
  const isChange = useCallback((e: EventRow) => changed.has(e.seq), [changed])
  // Worst finding per step, for timeline flags and the step pane.
  const flags = useMemo(() => {
    const m = new Map<number, Severity>()
    for (const f of findings) {
      const cur = m.get(f.seq)
      if (!cur || SEVERITY_RANK[f.severity] > SEVERITY_RANK[cur]) m.set(f.seq, f.severity)
    }
    return m
  }, [findings])
  const flagged = useCallback((e: EventRow) => (flags.get(e.seq) ? SEVERITY_RANK[flags.get(e.seq)!] >= 2 : false), [flags])

  const seek = useCallback(
    (i: number) => {
      const next = Math.min(Math.max(i, 0), last)
      stepRef.current = next
      setStep(next)
    },
    [last],
  )

  // Deep link: keep ?step= in the URL. Written with the History API after
  // the playhead rests, not through the router: a router update re-renders
  // the whole page, which is too slow at 16× on long sessions.
  useEffect(() => {
    const t = window.setTimeout(() => {
      const url = `/sessions/${id}${query({ step: step || undefined })}`
      if (location.pathname + location.search !== url) history.replaceState(history.state, '', url)
    }, 250)
    return () => window.clearTimeout(t)
  }, [step, id])

  // Playback: advance on a timer; stop at the end.
  useEffect(() => {
    if (!playing) return
    const t = window.setInterval(() => {
      if (stepRef.current >= last) {
        setPlaying(false)
        return
      }
      seek(stepRef.current + 1)
    }, 1000 / Number(speed))
    return () => window.clearInterval(t)
  }, [playing, speed, last, seek])

  const jump = useCallback(
    (i: number) => {
      setPlaying(false)
      seek(i)
    },
    [seek],
  )
  // Cache rebuilds in this session's own steps (subagents have their own replay).
  const rebuilds = useMemo(
    () => (cache.data?.latest ?? []).filter((b) => b.session_id === x.id).sort((a, b) => a.seq - b.seq),
    [cache.data, x.id],
  )
  // One dot on the timeline per rebuild.
  const marks = useMemo(() => new Map(rebuilds.map((b) => [b.seq, describeRebuild(b)])), [rebuilds])
  const nextRebuild = useCallback(() => {
    const at = events[stepRef.current]?.seq ?? -1
    const next = rebuilds.find((b) => b.seq > at) ?? rebuilds[0]
    const i = next ? events.findIndex((e) => e.seq === next.seq) : -1
    if (i >= 0) jump(i)
  }, [rebuilds, events, jump])

  const nextWhere = useCallback(
    (pred: (e: EventRow) => boolean, dir: 1 | -1 = 1) => {
      for (let i = stepRef.current + dir; i >= 0 && i <= last; i += dir) if (pred(events[i])) return seek(i)
    },
    [events, last, seek],
  )

  // The step with the biggest context window.
  const peakAt = useMemo(() => {
    let best = -1
    events.forEach((e, i) => {
      if (e.context_tokens != null && (best < 0 || e.context_tokens > (events[best].context_tokens ?? 0))) best = i
    })
    return best
  }, [events])
  const [costOpen, setCostOpen] = useState(false)
  // Steps of each part of the time strip, fetched once per part.
  const parts = useRef(new Map<string, BehindStep[]>())
  const pickPart = useCallback(
    async (what: string) => {
      let rows = parts.current.get(what)
      if (!rows) {
        const b = await getJson<Behind>(`/api/behind${query({ what, session: id })}`).catch(() => null)
        if (!b) return
        rows = b.items.filter((r) => r.session_id === id).sort((a, c) => a.seq - c.seq)
        parts.current.set(what, rows)
      }
      // The first one after the playhead, or the first of all when there is none.
      const at = events[stepRef.current]?.seq ?? -1
      const next = rows.find((r) => r.seq > at) ?? rows[0]
      const i = next ? events.findIndex((e) => e.seq === next.seq) : -1
      if (i >= 0) jump(i)
    },
    [id, events, jump],
  )
  const scrollToFiles = useCallback(() => document.getElementById('session-files')?.scrollIntoView({ block: 'start' }), [])

  // Keyboard: space, arrows, J/K, Shift for changes, E for errors.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target instanceof Element ? e.target : null
      if (t?.closest('input, textarea, [contenteditable], [cmdk-root]') || e.metaKey || e.ctrlKey || e.altKey) return
      const k = e.key
      if (k === ' ') setPlaying((p) => !p)
      else if ((k === 'ArrowRight' || k === 'j') && e.shiftKey) nextWhere(isChange)
      else if ((k === 'ArrowLeft' || k === 'k') && e.shiftKey) nextWhere(isChange, -1)
      else if (k === 'J') nextWhere(isChange)
      else if (k === 'K') nextWhere(isChange, -1)
      else if (k === 'ArrowRight' || k === 'j') seek(stepRef.current + 1)
      else if (k === 'ArrowLeft' || k === 'k') seek(stepRef.current - 1)
      else if (k === 'e') nextWhere((ev) => ev.is_error)
      else if (k === 'r') nextWhere(flagged)
      else if (k === 'Home') seek(0)
      else if (k === 'End') seek(last)
      else if (k === '1' || k === '2' || k === '3') setSpeed(SPEEDS[Number(k) - 1].value)
      else if (k === '+' || k === '=') setZoom((z) => ZOOMS[Math.min(ZOOMS.indexOf(z) + 1, ZOOMS.length - 1)])
      else if (k === '-') setZoom((z) => ZOOMS[Math.max(ZOOMS.indexOf(z) - 1, 0)])
      else return
      e.preventDefault()
      if (k !== ' ') setPlaying(false)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [nextWhere, seek, last, isChange, flagged])

  const title = oneLine(x.title ?? x.first_prompt ?? 'Untitled session')
  const cur = events[step]

  return (
    <div className={s.page}>
      <Link href={x.parent_id ? `/sessions/${x.parent_id}` : '/'} className={s.back}>
        <ArrowLeft aria-hidden /> {x.parent_id ? 'Parent session' : 'Overview'}
      </Link>

      <header className={s.head}>
        <div>
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
            {prs.map((p) => (
              <a key={p.url} href={p.url} target="_blank" rel="noreferrer" title={p.repo ?? p.url}>
                <GitPullRequest aria-hidden /> {p.number != null ? `PR #${p.number}` : 'Pull request'}
              </a>
            ))}
            {x.service_tier && (
              <span title="Codex service tier">
                <Gauge aria-hidden /> {x.service_tier} tier
              </span>
            )}
            {entrypointLabel(x.entrypoint) && (
              <span title={`Started from ${x.entrypoint}`}>
                <MonitorSmartphone aria-hidden /> {entrypointLabel(x.entrypoint)}
              </span>
            )}
            {x.started_at && (
              <span>
                <CalendarClock aria-hidden /> {formatDate(x.started_at)}, {formatTime(x.started_at)}
              </span>
            )}
          </div>
        </div>
        <div className={s.facts}>
          <span className={s.headActions}>
            <ResumeButton agent={x.agent} id={x.id} cwd={x.project_path} />
            <ExportLinks id={x.id} />
            <ReportCardButton build={(redact) => sessionCard(data, redact)} filename={`agentcam-session-${id.split('/').pop()}.png`} />
          </span>
          <span>
            <b>{formatHours(x.active_ms)}</b>agent time
          </span>
          {files.length > 0 ? (
            <button type="button" className={s.metaButton} onClick={scrollToFiles} title="Go to the files list">
              <b>{formatNumber(x.file_count)}</b>files changed
            </button>
          ) : (
            <span>
              <b>{formatNumber(x.file_count)}</b>files changed
            </span>
          )}
          {x.error_count > 0 ? (
            <button type="button" className={s.metaButton} onClick={() => nextWhere((e) => e.is_error)} title="Go to the next step that errored">
              <b>{formatNumber(x.error_count)}</b>errored
            </button>
          ) : (
            <span>
              <b>{formatNumber(x.error_count)}</b>errored
            </span>
          )}
          {cost.data ? (
            <button type="button" className={s.metaButton} onClick={() => setCostOpen(true)} title={costSummary(cost.data)}>
              <b>{x.est_cost_usd != null ? formatCost(x.est_cost_usd) + (x.cost_unpriced ? '+' : '') : x.cost_usd != null ? formatCost(x.cost_usd) : '–'}</b>
              est. cost
            </button>
          ) : (
            <span title={x.cost_unpriced ? 'Some calls use a model with no price; add it to prices.yaml' : 'Estimated at API list prices'}>
              <b>{x.est_cost_usd != null ? formatCost(x.est_cost_usd) + (x.cost_unpriced ? '+' : '') : x.cost_usd != null ? formatCost(x.cost_usd) : '–'}</b>
              est. cost
            </span>
          )}
          {peakAt >= 0 && x.peak_context ? (
            <button type="button" className={s.metaButton} onClick={() => jump(peakAt)} title="Go to the step with the biggest context window">
              <b>{formatCompact(x.peak_context)}</b>peak context
            </button>
          ) : (
            <span>
              <b>{x.peak_context ? formatCompact(x.peak_context) : '–'}</b>peak context
            </span>
          )}
          {lasted && (
            <Link
              href="/lasted"
              title={`${formatNumber(lasted.kept)} of ${formatNumber(lasted.added)} added lines are still in the code; ${formatNumber(lasted.reworked)} rewritten by agents, ${formatNumber(lasted.changed)} changed since`}
            >
              <b>{Math.round((100 * lasted.kept) / lasted.added)}%</b>still in code
            </Link>
          )}
          {rebuilds.length > 0 && (
            <button
              type="button"
              className={s.metaButton}
              onClick={nextRebuild}
              title={[...rebuilds.slice(0, 8).map((b) => `Step ${b.seq + 1}: ${describeRebuild(b)}`), 'Click to go to the next one'].join('\n')}
            >
              <b>{rebuilds.length}</b>cache {rebuilds.length === 1 ? 'rebuild' : 'rebuilds'}
            </button>
          )}
          {x.wasted_ms > 0 && (
            <span title="Agent time on retry loops, error streaks and reverts">
              <b>{formatHours(x.wasted_ms)}</b>in retries
            </span>
          )}
        </div>
      </header>
      {time.data && <TimeStrip data={time.data} onPick={pickPart} />}
      {costOpen && cost.data && (
        <Drawer title="Est. cost" label="Estimated cost of this session" onClose={() => setCostOpen(false)}>
          <div style={{ padding: 'var(--s-4)' }}>
            <CostPanel data={cost.data} />
          </div>
        </Drawer>
      )}

      {events.length === 0 ? (
        <Panel>
          <EmptyState title="No steps recorded in this session" />
        </Panel>
      ) : (
        <>
          <div className={s.player}>
            <Track events={events} current={step} onSeek={jump} zoom={zoom} flags={flags} marks={marks} />
            <div className={s.controls}>
              <Tooltip content={<>Previous <Kbd keys={['left']} /></>}>
                <Button icon variant="ghost" aria-label="Previous step" onClick={() => seek(step - 1)} disabled={step === 0}>
                  <ChevronLeft aria-hidden />
                </Button>
              </Tooltip>
              <Tooltip content={<>{playing ? 'Pause' : 'Play'} <Kbd keys={['space']} /></>}>
                <Button
                  icon
                  variant="primary"
                  aria-label={playing ? 'Pause' : 'Play'}
                  onClick={() => {
                    if (!playing && step >= last) seek(0)
                    setPlaying(!playing)
                  }}
                >
                  {playing ? <Pause aria-hidden /> : <Play aria-hidden />}
                </Button>
              </Tooltip>
              <Tooltip content={<>Next <Kbd keys={['right']} /></>}>
                <Button icon variant="ghost" aria-label="Next step" onClick={() => seek(step + 1)} disabled={step === last}>
                  <ChevronRight aria-hidden />
                </Button>
              </Tooltip>
              <Tooltip content={<>Next file change <Kbd keys={['shift', 'right']} /></>}>
                <Button icon variant="ghost" aria-label="Next file change" onClick={() => nextWhere(isChange)}>
                  <SkipForward aria-hidden />
                </Button>
              </Tooltip>
              {x.error_count > 0 && (
                <Tooltip content={<>Next step that errored <Kbd keys={['e']} /></>}>
                  <Button icon variant="ghost" aria-label="Next step that errored" onClick={() => nextWhere((e) => e.is_error)}>
                    <TriangleAlert aria-hidden style={{ color: 'var(--status-critical)' }} />
                  </Button>
                </Tooltip>
              )}
              <Segmented label="Speed" value={speed} onChange={setSpeed} options={SPEEDS} />
              <span className={s.position}>
                <b>
                  {step + 1} / {events.length}
                </b>
                {cur?.ts && <span className={s.hideSmall}>{formatTime(cur.ts)}</span>}
              </span>
              <span className={s.spacer} />
              <span className={s.hideSmall} style={{ display: 'inline-flex', gap: 2 }}>
                <Tooltip content={<>Zoom out <Kbd keys={['-']} /></>}>
                  <Button icon size="sm" variant="ghost" aria-label="Zoom out" onClick={() => setZoom(ZOOMS[Math.max(ZOOMS.indexOf(zoom) - 1, 0)])} disabled={zoom === 1}>
                    <ZoomOut aria-hidden />
                  </Button>
                </Tooltip>
                <Tooltip content={<>Zoom in <Kbd keys={['+']} /></>}>
                  <Button icon size="sm" variant="ghost" aria-label="Zoom in" onClick={() => setZoom(ZOOMS[Math.min(ZOOMS.indexOf(zoom) + 1, ZOOMS.length - 1)])} disabled={zoom === ZOOMS.at(-1)}>
                    <ZoomIn aria-hidden />
                  </Button>
                </Tooltip>
              </span>
              <Tooltip
                content={
                  <span style={{ display: 'grid', gap: 4 }}>
                    <span>
                      <Kbd keys={['j']} /> <Kbd keys={['k']} /> next, previous
                    </span>
                    <span>
                      <Kbd keys={['shift', 'j']} /> next file change
                    </span>
                    <span>
                      <Kbd keys={['e']} /> next error
                    </span>
                    <span>
                      <Kbd keys={['r']} /> next risk
                    </span>
                    <span>
                      <Kbd keys={['1']} /> <Kbd keys={['2']} /> <Kbd keys={['3']} /> speed
                    </span>
                  </span>
                }
                side="bottom"
              >
                <Button icon size="sm" variant="ghost" aria-label="Keyboard shortcuts" className={s.hideSmall}>
                  <Keyboard aria-hidden />
                </Button>
              </Tooltip>
            </div>
          </div>

          <div className={s.body}>
            <Panel
              title={view === 'trace' ? 'Trace' : view === 'turns' ? `${turns.length} turns` : 'Transcript'}
              icon={view === 'trace' ? <ChartNoAxesGantt /> : view === 'turns' ? <MessagesSquare /> : <Rows3 />}
              flush
              actions={
                <Segmented
                  label="View"
                  value={view}
                  onChange={setView}
                  options={[
                    { value: 'transcript', label: 'Transcript' },
                    { value: 'turns', label: 'Turns' },
                    { value: 'trace', label: 'Trace' },
                  ]}
                />
              }
            >
              {view === 'trace' ? (
                <TraceView events={events} subagents={subagents} current={step} onSeek={jump} />
              ) : view === 'turns' ? (
                <TurnList turns={turns} events={events} current={step} onSeek={jump} />
              ) : (
                <Transcript events={events} turns={turns} current={step} onSeek={jump} />
              )}
            </Panel>
            <div className={`${s.column} ${s.stickyCol}`}>
              <Panel flush>
                <div className={s.scroll} style={{ padding: 16 }}>
                  <StepDetail
                    sessionId={id}
                    event={cur}
                    index={step}
                    total={events.length}
                    root={x.project_root}
                    agent={x.agent}
                    findings={findings.filter((f) => f.seq === cur?.seq)}
                    ledTo={findings.filter((f) => f.related_seq != null && f.related_seq === cur?.seq && f.seq !== cur?.seq)}
                    onJump={jump}
                    onReviewed={onReviewed}
                  />
                </div>
              </Panel>
              {events.some((e) => e.context_tokens != null) && (
                <Panel title="Context window" icon={<Gauge />}>
                  <ContextChart events={events} current={step} onSeek={jump} />
                </Panel>
              )}
              {patterns.length > 0 && (
                <Panel title={`Retries and reverts · ${patterns.length}`} icon={<RotateCcw />} flush>
                  <div className={s.scroll} style={{ maxHeight: 220 }}>
                    {patterns.map((p, i) => (
                      <button
                        key={i}
                        type="button"
                        className={`${s.file} ${step >= p.start_seq && step <= p.end_seq ? s.fileOn : ''}`}
                        onClick={() => jump(p.start_seq)}
                        title={`Steps ${p.start_seq + 1}–${p.end_seq + 1}`}
                      >
                        <RotateCcw aria-hidden style={{ width: 13, height: 13, flex: 'none', color: p.kind === 'edit_churn' ? 'var(--text-3)' : 'var(--status-serious)' }} />
                        <span className={s.filePath} style={{ direction: 'ltr', fontFamily: 'var(--font-sans)' }}>
                          {p.title}
                        </span>
                        {p.wasted_ms > 0 && <span className="num">{formatDuration(p.wasted_ms)}</span>}
                      </button>
                    ))}
                  </div>
                </Panel>
              )}
              {commits.length > 0 && (
                <Panel title={`Commits · ${commits.length}`} icon={<GitCommitHorizontal />} flush>
                  {commits.map((c) => (
                    <button key={c.seq} type="button" className={`${s.file} ${c.seq === cur?.seq ? s.fileOn : ''}`} onClick={() => jump(c.seq)}>
                      <span className="mono" style={{ flex: 'none', color: 'var(--text-3)' }}>
                        {c.hash ?? '·······'}
                      </span>
                      <span className={s.filePath} style={{ direction: 'ltr', fontFamily: 'var(--font-sans)' }}>
                        {c.message ?? '(no message)'}
                      </span>
                    </button>
                  ))}
                </Panel>
              )}
              {findings.length > 0 && (
                <Panel
                  title={`Risks · ${findings.filter((f) => !f.review).length} to review${findings.some((f) => f.review) ? ` of ${findings.length}` : ''}`}
                  icon={<ShieldAlert />}
                  flush
                >
                  <div className={s.scroll} style={{ maxHeight: 260 }}>
                    {findings.map((f, i) => (
                      <button
                        key={i}
                        type="button"
                        className={`${s.file} ${f.seq === cur?.seq ? s.fileOn : ''} ${f.review ? s.looked : ''}`}
                        onClick={() => jump(f.seq)}
                        title={f.review === 'reviewed' ? 'Looked at' : f.review === 'muted' ? 'Rule muted' : undefined}
                      >
                        <ShieldAlert aria-hidden style={{ width: 13, height: 13, flex: 'none', color: SEVERITY_COLOR[f.severity] }} />
                        <span style={{ fontFamily: 'var(--font-sans)', flex: 'none' }}>{f.title}</span>
                        <span className={s.filePath}>
                          <bdi>{f.excerpt}</bdi>
                        </span>
                      </button>
                    ))}
                  </div>
                </Panel>
              )}
              {follow && follow.outcomes.some((o) => o.status !== 'not_applicable') && (
                <Panel title="Instructions" icon={<ScrollText />} flush>
                  <div className={s.follow}>
                    {follow.rules.map((r) => {
                      const o = follow.outcomes.find((x) => x.rule_id === r.id)
                      if (!o || o.status === 'not_applicable') return null
                      const done = o.status === 'followed'
                      const body = (
                        <>
                          {done ? (
                            <CircleCheck aria-hidden style={{ color: 'var(--status-good)' }} />
                          ) : (
                            <CircleMinus aria-hidden style={{ color: 'var(--status-warning)' }} />
                          )}
                          <span>
                            <CodeText text={ruleText(r)} />
                            {o.detail && (
                              <span className={s.followDetail}>
                                <CodeText text={o.detail} />
                              </span>
                            )}
                          </span>
                        </>
                      )
                      return o.seq != null && o.session_id === id ? (
                        <button key={r.id} type="button" className={s.followRow} onClick={() => jump(o.seq!)}>
                          {body}
                        </button>
                      ) : (
                        <div key={r.id} className={s.followRow}>
                          {body}
                        </div>
                      )
                    })}
                  </div>
                </Panel>
              )}
              {plan && <PlanPanel plan={plan} step={step} onSeek={jump} />}
              {files.length > 0 && (
                <Panel id="session-files" title="Files" icon={<FileStack />} actions={<EditorPicker />} flush>
                  <Files files={files} root={x.project_root} current={step} onSeek={jump} />
                </Panel>
              )}
              {subagents.length > 0 && <SubagentTree parent={x} subagents={subagents} onOpen={(sid) => navigate(`/sessions/${sid}`)} />}
            </div>
          </div>
        </>
      )}
    </div>
  )
}

/** Every step as a readable list that follows the playhead. */
function Transcript({
  events,
  turns,
  current,
  onSeek,
}: {
  events: EventRow[]
  turns: TurnRow[]
  current: number
  onSeek: (i: number) => void
}) {
  const turnAt = useMemo(() => new Map(turns.map((t) => [t.seq, t])), [turns])
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const el = box.current?.querySelector<HTMLElement>(`[data-i="${current}"]`)
    const b = box.current
    if (!el || !b) return
    // Scroll inside the transcript only, never the page. The box is the
    // item's offset parent, so offsetTop is already relative to it.
    const top = el.offsetTop
    if (top < b.scrollTop + 40 || top + el.offsetHeight > b.scrollTop + b.clientHeight - 40) {
      b.scrollTo({ top: top - b.clientHeight / 3, behavior: 'smooth' })
    }
  }, [current])

  return (
    <div className={s.scroll} ref={box} style={{ position: 'relative' }}>
      {events.map((e, i) => {
        const t = turnAt.get(e.seq)
        return (
          <Fragment key={e.seq}>
            {t && <TurnHeader t={t} />}
            <Row e={e} i={i} state={i === current ? 'on' : i > current ? 'future' : 'past'} onSeek={onSeek} />
          </Fragment>
        )
      })}
    </div>
  )
}

/** One transcript row. Memoized: moving the playhead re-renders only the
 * rows whose state changed, not the whole list. */
const Row = memo(function Row({
  e,
  i,
  state,
  onSeek,
}: {
  e: EventRow
  i: number
  state: 'past' | 'on' | 'future'
  onSeek: (i: number) => void
}) {
  return (
    <button
      type="button"
      data-i={i}
      className={`${s.item} ${state === 'on' ? s.itemOn : ''} ${state === 'future' ? s.itemFuture : ''}`}
      onClick={() => onSeek(i)}
    >
      <TranscriptItem e={e} />
    </button>
  )
})

function TranscriptItem({ e }: { e: EventRow }) {
  if (e.kind === 'prompt') {
    return (
      <span className={s.itemBody}>
        <span className={s.who} style={{ display: 'block' }}>
          You
        </span>
        <span className={`${s.text} ${s.prompt}`}>{e.text ?? e.title}</span>
      </span>
    )
  }
  if (e.kind === 'response') {
    return (
      <span className={s.itemBody}>
        <Markdown className={s.clamp} text={e.text ?? e.title} />
      </span>
    )
  }
  if (e.kind === 'thinking') {
    return (
      <span className={s.itemBody}>
        <span className={s.thinking} style={{ display: 'block' }}>
          Thinking{e.duration_ms ? ` ${formatDuration(e.duration_ms)}` : ''} · {e.title}
        </span>
      </span>
    )
  }
  return (
    <span className={`${s.itemBody} ${s.toolLine}`}>
      <KindIcon kind={e.kind} category={e.category} isError={e.is_error} tool={e.tool_name} />
      <span className={s.toolTitle}>{e.title}</span>
      {e.approval && approvalInfo(e.approval).stopped && (
        <span className={s.toolDur} title={approvalInfo(e.approval).label}>
          {createElement(approvalInfo(e.approval).icon, { 'aria-hidden': true, style: { width: 12, height: 12, color: approvalInfo(e.approval).color, verticalAlign: -2 } })}{' '}
          {approvalInfo(e.approval).short}
        </span>
      )}
      {e.duration_ms != null && e.duration_ms > 500 && <span className={s.toolDur}>{formatDuration(e.duration_ms)}</span>}
    </span>
  )
}

/** A line above each prompt: which turn, how long it ran, and with what. */
const TurnHeader = memo(function TurnHeader({ t }: { t: TurnRow }) {
  const bits = [
    t.duration_ms != null ? formatDuration(t.duration_ms).replace(/ \d+s$/, '') : null,
    `${t.steps} step${t.steps === 1 ? '' : 's'}`,
    t.cost_usd != null ? formatCost(t.cost_usd) : null,
    t.model,
    t.effort ? `${t.effort} effort` : null,
    modeLabel(t.mode) ? `${modeLabel(t.mode)} mode` : null,
  ].filter(Boolean)
  return (
    <div className={s.turnHead}>
      <span className={s.turnN}>Turn {t.n + 1}</span>
      <span>{bits.join(' · ')}</span>
      {t.wait_ms != null && t.wait_ms > 60_000 && t.wait_ms < 8 * 3_600_000 && (
        <span className={s.turnWait}>you replied after {formatHours(t.wait_ms)}</span>
      )}
    </div>
  )
})

/** Every turn as a row; click one to jump to its prompt. */
function TurnList({
  turns,
  events,
  current,
  onSeek,
}: {
  turns: TurnRow[]
  events: EventRow[]
  current: number
  onSeek: (i: number) => void
}) {
  const index = useMemo(() => new Map(events.map((e, i) => [e.seq, i])), [events])
  const cur = events[current]?.seq ?? 0
  if (!turns.length) return <p className={s.note} style={{ padding: 16 }}>No prompts in this session.</p>
  const longest = Math.max(1, ...turns.map((t) => t.duration_ms ?? 0))
  return (
    <div className={s.scroll}>
      {turns.map((t) => {
        const on = cur >= t.seq && cur <= t.end_seq
        return (
          <button
            type="button"
            key={t.n}
            className={`${s.turnRow} ${on ? s.itemOn : ''}`}
            onClick={() => onSeek(index.get(t.seq) ?? 0)}
          >
            <span className={s.turnN}>{t.n + 1}</span>
            <span className={s.turnMain}>
              <span className={s.turnPrompt}>{t.prompt || 'Prompt'}</span>
              <span className={s.turnMeta}>
                {[
                  `${t.steps} steps`,
                  t.errors ? `${t.errors} errored` : null,
                  t.cost_usd != null ? formatCost(t.cost_usd) : null,
                  t.thinking_ms ? `${formatDuration(t.thinking_ms)} thinking` : null,
                  t.model,
                  t.effort,
                  modeLabel(t.mode),
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </span>
            <span className={s.turnLen}>
              <span className={s.turnBar} style={{ width: `${Math.max(4, ((t.duration_ms ?? 0) / longest) * 100)}%` }} />
              <span>{t.duration_ms != null ? formatDuration(t.duration_ms).replace(/ \d+s$/, '') : '–'}</span>
            </span>
          </button>
        )
      })}
    </div>
  )
}

type Step = {
  seq: number
  ts: string | null
  kind: EventRow['kind']
  tool_name: string | null
  category: EventRow['category']
  title: string
  body: string
  input: string | null
  is_error: boolean
  duration_ms: number | null
  truncated: boolean
  touches: { path: string; action: string; diff: string | null; lasted: Lasted | null }[]
}

const rel = (path: string, root: string | null) => (root && path.startsWith(root + '/') ? path.slice(root.length + 1) : path)

function pretty(json: string | null): string | null {
  if (!json) return null
  try {
    const v = JSON.parse(json)
    if (typeof v === 'string') return v
    return JSON.stringify(v, null, 2)
  } catch {
    return json
  }
}

/** Everything about the step under the playhead. */
function StepDetail({
  sessionId,
  event,
  index,
  total,
  root,
  agent,
  findings,
  ledTo,
  onJump,
  onReviewed,
}: {
  sessionId: string
  event?: EventRow
  index: number
  total: number
  root: string | null
  agent: string
  findings: FindingRow[]
  /** Findings on later steps whose chain starts at this step. */
  ledTo: FindingRow[]
  onJump: (seq: number) => void
  onReviewed: () => void
}) {
  const [blocking, setBlocking] = useState(false)
  const { data: st, error } = useApi<Step>(event ? `/api/step${query({ session: sessionId, seq: event.seq })}` : null)
  if (!event) return null
  const fresh = st && st.seq === event.seq ? st : null
  const g = GROUPS[groupOf(event.kind, event.category, event.is_error)]
  const tool = event.kind === 'tool_call'

  return (
    <div>
      <div className={s.detailHead}>
        <KindIcon kind={event.kind} category={event.category} isError={event.is_error} />
        <span style={{ color: 'var(--text-1)', fontWeight: 520 }}>{kindLabel(event.kind, event.category, event.tool_name)}</span>
        {event.tool_name && <span className={s.mono}>{event.tool_name}</span>}
        <span>·</span>
        <span>
          Step {index + 1} of {total}
        </span>
        {event.ts && <span>· {formatTime(event.ts)}</span>}
        {event.duration_ms != null && <span>· took {formatDuration(event.duration_ms)}</span>}
        {event.approval && (
          <Badge icon={createElement(approvalInfo(event.approval).icon)} color={approvalInfo(event.approval).color}>
            {approvalInfo(event.approval).label}
          </Badge>
        )}
        {event.via && <Badge icon={<Puzzle />}>{viaLabel(event.via)}</Badge>}
        {event.is_error && (
          <Badge icon={<TriangleAlert />} color="var(--status-critical)">
            Errored
          </Badge>
        )}
        <span className="sr-only">{g.label}</span>
        <CopyButton
          className={s.stepLink}
          text={`${window.location.origin}/sessions/${sessionId}?step=${event.seq}`}
          label="Copy link to this step"
        />
      </div>

      {findings.length > 0 && (
        <div className={s.findings}>
          {findings.map((f, i) => (
            <div key={i} className={s.finding}>
              <SeverityBadge severity={f.severity} />
              <span className={s.findingTitle}>{f.title}</span>
              {f.related_seq != null && f.related_seq !== f.seq && (
                <button type="button" className={s.chainLink} onClick={() => onJump(f.related_seq!)}>
                  <CornerLeftUp aria-hidden /> follows step {f.related_seq + 1}
                </button>
              )}
              <span className={`${s.mono} ${s.findingExcerpt}`}>{f.excerpt}</span>
              <FindingReview
                sessionId={sessionId}
                seq={f.seq}
                ruleId={f.rule_id}
                review={f.review}
                project={root}
                onDone={onReviewed}
              />
            </div>
          ))}
        </div>
      )}
      {ledTo.length > 0 && (
        <div className={s.findings}>
          {ledTo.map((f, i) => (
            <div key={i} className={s.finding}>
              <Link2 aria-hidden className={s.chainIcon} />
              <span className={s.findingNote}>This step starts a chain:</span>
              <button type="button" className={s.chainLink} onClick={() => onJump(f.seq)}>
                {f.title} at step {f.seq + 1} <CornerRightDown aria-hidden />
              </button>
            </div>
          ))}
        </div>
      )}

      {tool && (
        <div className={s.titleRow}>
          <div className={s.detailTitle}>{event.title}</div>
          <CopyButton text={event.title} label={event.category === 'command' ? 'Copy command' : 'Copy'} />
        </div>
      )}
      {tool && event.tool_name && (agent === 'claude-code' || agent === 'codex') && (
        <div style={{ marginTop: -6, marginBottom: 12 }}>
          <Button size="sm" variant={findings.length ? 'default' : 'ghost'} onClick={() => setBlocking(true)}>
            <Ban aria-hidden /> Block next time
          </Button>
        </div>
      )}
      {blocking && event.tool_name && (
        <BlockDialog
          sessionId={sessionId}
          seq={event.seq}
          tool={event.tool_name}
          // Start from the risky part of the step (say, the force push in a
          // longer command). Masked secrets can't be matched, so skip those.
          // Chain findings describe two steps, so they aren't a pattern either.
          title={findings.find((f) => f.related_seq == null && !f.excerpt.includes('…'))?.excerpt ?? event.title}
          why={findings[0]?.title}
          onClose={() => setBlocking(false)}
        />
      )}

      {error && <p className={s.note}>Could not load this step: {error}</p>}
      {!fresh && !error && (
        <div style={{ display: 'grid', gap: 10, marginTop: 16 }}>
          <Skeleton width="70%" />
          <Skeleton width="50%" />
          <Skeleton height={120} />
        </div>
      )}
      {fresh && <StepBody st={fresh} root={root} />}
    </div>
  )
}

function StepBody({ st, root }: { st: Step; root: string | null }) {
  if (st.kind === 'response') {
    return <Markdown text={st.body || st.title} className={s.detailMd} />
  }
  if (st.kind === 'prompt' || st.kind === 'system') {
    return (
      <>
        <p className={s.proseText} style={{ marginTop: 12 }}>{st.body || st.title}</p>
        {st.kind === 'prompt' && <CopyButton text={st.body || st.title} label="Copy prompt" />}
      </>
    )
  }
  if (st.kind === 'thinking') {
    return <p className={s.thinkingText} style={{ marginTop: 12 }}>{st.body}</p>
  }

  const changes = st.touches.filter((t) => t.action !== 'read')
  const input = pretty(st.input)
  const showInput = st.category !== 'command' && st.category !== 'read' && !(changes.length && changes.some((t) => t.diff))

  return (
    <>
      {changes.map((t, i) => (
        <div key={i}>
          <div className={s.touch}>
            <ActionTag action={t.action} />
            <span>{rel(t.path, root)}</span>
            {t.action !== 'delete' && <OpenInEditor path={t.path} line={firstLine(t.diff)} />}
          </div>
          {t.lasted && t.lasted.added > 0 && (
            <div className={s.lasted}>
              <LastedBar c={t.lasted} height={6} label="Lines this change added" />
              <span>
                <b>
                  {formatNumber(t.lasted.kept)} of {formatNumber(t.lasted.added)}
                </b>{' '}
                added lines still in the file
                {t.lasted.reworked > 0 && ` · ${formatNumber(t.lasted.reworked)} rewritten by agents`}
                {t.lasted.changed > 0 && ` · ${formatNumber(t.lasted.changed)} changed since`}
              </span>
            </div>
          )}
          {t.diff ? (
            <DiffView diff={t.diff} created={t.action === 'create' && !t.diff.startsWith('@@')} />
          ) : t.action !== 'delete' ? (
            <p className={s.note}>No diff in the log for this change (made by a shell command).</p>
          ) : null}
        </div>
      ))}
      {showInput && input && (
        <>
          <div className={s.labelRow}>
            <span className={s.label}>Input</span>
            <CopyButton text={input} label="Copy input" />
          </div>
          <pre className={s.pre}>{input}</pre>
        </>
      )}
      {st.body && (
        <>
          <div className={s.labelRow}>
            <span className={s.label}>{st.is_error ? 'Error output' : 'Output'}</span>
            <CopyButton text={st.body} label="Copy output" />
          </div>
          <pre className={`${s.pre} ${st.is_error ? s.preError : ''}`}>{st.body}</pre>
        </>
      )}
      {!st.body && !changes.length && <p className={s.note}>No output recorded.</p>}
      {st.truncated && <p className={s.note}>Cut to 32 KB in the index. The full text is in the agent's log file.</p>}
    </>
  )
}

const ACTION_COLORS: Record<string, string> = {
  read: 'var(--kind-look)',
  edit: 'var(--kind-change)',
  create: 'var(--status-good)',
  delete: 'var(--status-critical)',
}
const ACTION_LETTER: Record<string, string> = { read: 'R', edit: 'M', create: 'A', delete: 'D' }

function ActionTag({ action }: { action: string }) {
  return (
    <Tooltip content={action[0].toUpperCase() + action.slice(1)}>
      <span className={s.act} style={{ background: ACTION_COLORS[action] }} aria-label={action}>
        {ACTION_LETTER[action] ?? '?'}
      </span>
    </Tooltip>
  )
}

/** Files the session touched. Click to jump to the next step that touched it. */
function Files({
  files,
  root,
  current,
  onSeek,
}: {
  files: SessionDetail['files']
  root: string | null
  current: number
  onSeek: (i: number) => void
}) {
  const [showReads, setShowReads] = useState(false)
  const list = useMemo(() => {
    const m = new Map<string, { path: string; actions: Set<string>; seqs: number[] }>()
    for (const f of files) {
      const e = m.get(f.path) ?? { path: f.path, actions: new Set(), seqs: [] }
      e.actions.add(f.action)
      e.seqs.push(f.seq)
      m.set(f.path, e)
    }
    return [...m.values()]
  }, [files])
  const changed = list.filter((f) => [...f.actions].some((a) => a !== 'read'))
  const reads = list.filter((f) => !changed.includes(f))
  const at = (f: (typeof list)[number]) => f.seqs.includes(current)

  const row = (f: (typeof list)[number]) => (
    <div key={f.path} className={s.fileRow}>
      <button
        type="button"
        className={`${s.file} ${at(f) ? s.fileOn : ''}`}
        title={`${f.path}\n${f.seqs.length} ${f.seqs.length === 1 ? 'step' : 'steps'}`}
        onClick={() => onSeek(f.seqs.find((q) => q > current) ?? f.seqs[0])}
      >
        <span className={s.actions}>
          {['create', 'edit', 'delete', 'read']
            .filter((a) => f.actions.has(a))
            .map((a) => (
              <span key={a} className={s.act} style={{ background: ACTION_COLORS[a] }}>
                {ACTION_LETTER[a]}
              </span>
            ))}
        </span>
        <span className={s.filePath}>
          <bdi>{rel(f.path, root)}</bdi>
        </span>
      </button>
      {!(f.actions.size === 1 && f.actions.has('delete')) && <OpenInEditor path={f.path} />}
    </div>
  )

  return (
    <div className={s.scroll} style={{ maxHeight: 320 }}>
      {changed.length > 0 && <div className={s.groupLabel}>Changed · {changed.length}</div>}
      {changed.map(row)}
      {reads.length > 0 && (
        <button type="button" className={s.groupLabel} style={{ border: 0, background: 'none', cursor: 'pointer', width: '100%', textAlign: 'left' }} onClick={() => setShowReads(!showReads)}>
          {showReads ? '▾' : '▸'} Only read · {reads.length}
        </button>
      )}
      {showReads && reads.map(row)}
    </div>
  )
}
