import { ArrowRight, CircleCheck, Inbox, Play, RefreshCw, Rows3, Settings2, TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { formatDuration } from '../lib/format'
import { GROUP_ORDER, GROUPS, kindLabel, type EventKind, type ToolCategory } from '../lib/kinds'
import { Badge } from '../ui/Badge'
import { Button } from '../ui/Button'
import { EventStrip } from '../ui/EventStrip'
import { Kbd } from '../ui/Kbd'
import { ListRow } from '../ui/ListRow'
import { AgentLabel, EmptyState, KindIcon, Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import { Segmented } from '../ui/Segmented'
import { Stat } from '../ui/Stat'
import { Tooltip } from '../ui/Tooltip'
import { SAMPLE_EVENTS, SAMPLE_SESSIONS } from './sample'
import s from './Styleguide.module.css'

const SECTIONS = ['Color', 'Event kinds', 'Type', 'Space', 'Components', 'Timeline', 'Motion'] as const
const slug = (t: string) => t.toLowerCase().replace(/\s+/g, '-')

/** Reads a CSS variable so swatches always show the real token value. */
function useToken(name: string) {
  const [v] = useState(() => getComputedStyle(document.documentElement).getPropertyValue(name).trim())
  return v
}

function Swatch({ token, name, hint }: { token: string; name: string; hint?: string }) {
  const value = useToken(token)
  return (
    <div className={s.swatch}>
      <div className={s.chip} style={{ background: `var(${token})` }} />
      <div className={s.swatchText}>
        <span className={s.swatchName}>{name}</span>
        <span className={s.swatchValue}>
          {token} · {value}
        </span>
        {hint && <span className={s.swatchHint}>{hint}</span>}
      </div>
    </div>
  )
}

const GROUP_MEMBERS: Record<string, [EventKind, ToolCategory?][]> = {
  you: [['prompt']],
  change: [['tool_call', 'command'], ['tool_call', 'edit'], ['tool_call', 'write']],
  look: [['tool_call', 'read'], ['tool_call', 'search'], ['tool_call', 'web'], ['tool_call', 'mcp'], ['tool_call', 'subagent']],
  agent: [['response'], ['thinking'], ['system']],
  error: [['tool_call', 'command']],
}

export function Styleguide() {
  const [range, setRange] = useState<'7d' | '30d' | 'all'>('30d')
  const [selected, setSelected] = useState(0)
  const [step, setStep] = useState<number | undefined>(8)
  const current = step != null ? SAMPLE_EVENTS[step] : null

  return (
    <div className={s.page}>
      <nav className={s.toc} aria-label="Styleguide sections">
        <span className={s.tocLabel}>Styleguide</span>
        {SECTIONS.map((t) => (
          <a key={t} href={`#${slug(t)}`}>
            {t}
          </a>
        ))}
      </nav>

      <div className={s.content}>
        <header className={s.intro}>
          <h1>Styleguide</h1>
          <p>Every token and component agentcam is built from. Dark, dense and calm: the data is the loud part.</p>
        </header>

        <section id="color" className={s.section}>
          <h2>Color</h2>
          <p className={s.rule}>
            Four surfaces, three line weights, three text colors. Text always uses a text color, never a data color.
          </p>
          <span className={s.sub}>Surfaces</span>
          <div className={s.swatches}>
            <Swatch token="--bg-0" name="Background" hint="The app" />
            <Swatch token="--bg-1" name="Surface" hint="Panels, charts" />
            <Swatch token="--bg-2" name="Raised" hint="Hover, controls" />
            <Swatch token="--bg-3" name="Selected" hint="Active, pressed" />
          </div>
          <span className={s.sub}>Lines and text</span>
          <div className={s.swatches}>
            <Swatch token="--line-1" name="Hairline" />
            <Swatch token="--line-2" name="Border" />
            <Swatch token="--line-3" name="Strong" />
            <Swatch token="--text-1" name="Text" hint="16:1 on surface" />
            <Swatch token="--text-2" name="Secondary" hint="7.4:1" />
            <Swatch token="--text-3" name="Muted" hint="5.6:1" />
          </div>
          <span className={s.sub}>Status (always with an icon)</span>
          <div className={s.swatches}>
            <Swatch token="--status-good" name="Good" />
            <Swatch token="--status-warning" name="Warning" />
            <Swatch token="--status-serious" name="Serious" />
            <Swatch token="--status-critical" name="Critical" />
          </div>
          <span className={s.sub}>Agents (always beside the name) and brand</span>
          <div className={s.swatches}>
            <Swatch token="--agent-claude-code" name="Claude Code" />
            <Swatch token="--agent-codex" name="Codex" />
            <Swatch token="--rec" name="Recording" hint="Logo and live dot only" />
          </div>
          <span className={s.sub}>Activity ramp</span>
          <div>
            <div className={s.heat}>
              {[0, 1, 2, 3, 4, 5].map((i) => (
                <span key={i} className={s.heatCell} style={{ background: `var(--heat-${i})` }} />
              ))}
            </div>
            <div className={s.heatLabels}>
              <span>None</span>
              <span>Most</span>
            </div>
          </div>
        </section>

        <section id="event-kinds" className={s.section}>
          <h2>Event kinds</h2>
          <p className={s.rule}>
            Every step lands in one of five groups. Color tells you the group at a glance; the icon tells you the exact
            kind. Three hues is the most that stay distinct for color-blind readers, so there will never be a fourth.
          </p>
          <div className={s.groupCards}>
            {GROUP_ORDER.map((g) => (
              <div key={g} className={s.groupCard}>
                <div className={s.groupBar} style={{ background: GROUPS[g].color }} />
                <span className={s.groupHead}>{GROUPS[g].label}</span>
                <span className={s.swatchHint}>{GROUPS[g].hint}</span>
                <div className={s.groupIcons}>
                  {GROUP_MEMBERS[g].map(([k, c]) => (
                    <Tooltip key={`${k}${c}`} content={kindLabel(k, c)}>
                      <KindIcon kind={k} category={c} isError={g === 'error'} />
                    </Tooltip>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>

        <section id="type" className={s.section}>
          <h2>Type</h2>
          <p className={s.rule}>
            Inter for the interface, JetBrains Mono for anything a machine wrote: commands, paths, code. Both ship inside
            the binary, so nothing loads from the network. The UI default is 13px.
          </p>
          <div>
            {[
              ['--fs-4xl', '40 / semibold', 'Six months of agents'],
              ['--fs-3xl', '28 / semibold', '14,203 tool calls'],
              ['--fs-2xl', '20 / semibold', 'Fix the login bug'],
              ['--fs-xl', '16 / regular', 'Search sessions, commands and files'],
              ['--fs-lg', '14 / regular', 'The login form rejects valid passwords.'],
              ['--fs-md', '13 / regular', 'Default interface text, list rows, buttons.'],
              ['--fs-sm', '12 / regular', 'Meta lines, timestamps, secondary labels.'],
              ['--fs-xs', '11 / medium', 'BADGES, KEY HINTS, SECTION LABELS'],
            ].map(([t, meta, sample]) => (
              <div key={t} className={s.typeRow}>
                <span className={s.typeMeta}>
                  {t}
                  <br />
                  {meta}
                </span>
                <span
                  className={s.typeSample}
                  style={{
                    fontSize: `var(${t})`,
                    fontWeight: t.match(/2xl|3xl|4xl/) ? 'var(--fw-semibold)' : undefined,
                    letterSpacing: t.match(/3xl|4xl/) ? '-0.025em' : undefined,
                  }}
                >
                  {sample}
                </span>
              </div>
            ))}
            <div className={s.typeRow}>
              <span className={s.typeMeta}>
                --font-mono
                <br />
                13 / regular
              </span>
              <code className={s.typeSample}>git commit -m "Stop trimming passwords" &amp;&amp; npm test -- --watch=false</code>
            </div>
          </div>
        </section>

        <section id="space" className={s.section}>
          <h2>Space and shape</h2>
          <p className={s.rule}>A 4px grid. Corners stay small; only overlays get the 12px radius.</p>
          <div className={s.spaceRow}>
            {[1, 2, 3, 4, 6, 8, 12, 16].map((n) => (
              <div key={n} className={s.spaceItem}>
                <span className={s.spaceBox} style={{ width: `var(--s-${n})`, height: `var(--s-${n})` }} />
                s-{n}
              </div>
            ))}
          </div>
          <div className={s.spaceRow}>
            {[1, 2, 3, 4].map((n) => (
              <div key={n} className={s.spaceItem}>
                <span className={s.radiusBox} style={{ borderRadius: `var(--r-${n})` }} />
                r-{n}
              </div>
            ))}
          </div>
        </section>

        <section id="components" className={s.section}>
          <h2>Components</h2>

          <span className={s.sub}>Buttons</span>
          <div className={s.row}>
            <Button variant="primary">
              <Play aria-hidden /> Replay
            </Button>
            <Button>
              <RefreshCw aria-hidden /> Re-index
            </Button>
            <Button variant="ghost">
              View all <ArrowRight aria-hidden />
            </Button>
            <Tooltip content="Settings">
              <Button icon variant="ghost" aria-label="Settings">
                <Settings2 aria-hidden />
              </Button>
            </Tooltip>
            <Button size="sm">Small</Button>
            <Button disabled>Disabled</Button>
          </div>

          <span className={s.sub}>Badges, agents, key hints</span>
          <div className={s.row}>
            <AgentLabel agent="claude-code" />
            <AgentLabel agent="codex" />
            <Badge>main</Badge>
            <Badge dot color="var(--status-good)">
              Indexed
            </Badge>
            <Badge icon={<TriangleAlert />} color="var(--status-critical)">
              2 errors
            </Badge>
            <Badge icon={<CircleCheck />} color="var(--status-good)">
              Tests passed
            </Badge>
            <Badge solid>318 steps</Badge>
            <Kbd keys={['mod', 'k']} />
            <Kbd keys={['shift', 'enter']} />
            <Kbd keys={['j']} />
          </div>

          <span className={s.sub}>Segmented control and tooltip</span>
          <div className={s.row}>
            <Segmented
              label="Time range"
              value={range}
              onChange={setRange}
              options={[
                { value: '7d', label: '7 days' },
                { value: '30d', label: '30 days' },
                { value: 'all', label: 'All time' },
              ]}
            />
            <Tooltip content={<>Open the command palette <Kbd keys={['mod', 'k']} /></>}>
              <Button>Hover or focus me</Button>
            </Tooltip>
          </div>

          <span className={s.sub}>Panels, list rows, stats</span>
          <div className={s.grid2}>
            <Panel
              title="Recent sessions"
              icon={<Rows3 />}
              flush
              actions={
                <Button size="sm" variant="ghost">
                  View all
                </Button>
              }
            >
              {SAMPLE_SESSIONS.map((x, i) => (
                <ListRow
                  key={x.title}
                  selected={i === selected}
                  onClick={() => setSelected(i)}
                  leading={<AgentLabel agent={x.agent} hideName />}
                  title={x.title}
                  meta={
                    <>
                      <span>{x.project}</span>
                      <span>·</span>
                      <span className="mono">{x.branch}</span>
                      <span>·</span>
                      <span>{x.when}</span>
                    </>
                  }
                  trailing={
                    <>
                      {x.errors > 0 && (
                        <Badge icon={<TriangleAlert />} color="var(--status-critical)">
                          {x.errors}
                        </Badge>
                      )}
                      <span className="num">{formatDuration(x.duration)}</span>
                    </>
                  }
                />
              ))}
            </Panel>
            <Panel title="Last 30 days">
              <div className={s.stats}>
                <Stat label="Sessions" value="139" sub="+12 this week" />
                <Stat label="Agent time" value="61h" sub="across 7 projects" />
                <Stat label="Tool calls" value="11.7k" sub="3,957 commands" />
                <Stat label="Files changed" value="412" sub="218 created" />
              </div>
            </Panel>
          </div>

          <span className={s.sub}>Loading and empty</span>
          <div className={s.grid2}>
            <Panel title="Loading">
              <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                <Skeleton width="60%" height={14} />
                <Skeleton width="85%" />
                <Skeleton width="40%" />
              </div>
            </Panel>
            <Panel flush>
              <EmptyState icon={<Inbox />} title="No sessions yet">
                Run a coding agent, then come back. agentcam picks up new logs on its own.
              </EmptyState>
            </Panel>
          </div>
        </section>

        <section id="timeline" className={s.section}>
          <h2>Timeline</h2>
          <p className={s.rule}>
            A session as one row of steps. Width grows with how long a step took. Hover for details, click to move the
            playhead. This is the first draft of the replay track.
          </p>
          <Panel title="Fix the login bug" icon={<Play />}>
            <EventStrip events={SAMPLE_EVENTS} current={step} onSelect={setStep} label="Session steps" />
            <div className={s.legend}>
              {GROUP_ORDER.map((g) => (
                <span key={g}>
                  <span className={s.legendDot} style={{ background: GROUPS[g].color }} />
                  {GROUPS[g].label}
                </span>
              ))}
            </div>
            {current && (
              <div className={s.stepDetail}>
                <KindIcon kind={current.kind} category={current.category} isError={current.isError} />
                <span className={s.swatchHint}>
                  Step {step! + 1} of {SAMPLE_EVENTS.length}
                </span>
                <span className={s.stepTitle}>{current.title}</span>
                {current.durationMs != null && (
                  <span className={`${s.swatchHint} num`}>{formatDuration(current.durationMs)}</span>
                )}
              </div>
            )}
          </Panel>
        </section>

        <section id="motion" className={s.section}>
          <h2>Motion</h2>
          <p className={s.rule}>
            Short and calm. Things respond to you; nothing moves on its own except the replay playhead. All motion turns
            off when your system asks for reduced motion.
          </p>
          <table className={s.motionTable}>
            <thead>
              <tr>
                <th>Token</th>
                <th>Value</th>
                <th>Use</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>--dur-1</td>
                <td>100ms</td>
                <td>Hover and press feedback</td>
              </tr>
              <tr>
                <td>--dur-2</td>
                <td>160ms</td>
                <td>Tooltips, playhead moves, small reveals</td>
              </tr>
              <tr>
                <td>--dur-3</td>
                <td>240ms</td>
                <td>Overlays and panels opening</td>
              </tr>
              <tr>
                <td>--ease-out</td>
                <td>cubic-bezier(0.2, 0.8, 0.2, 1)</td>
                <td>Everything that enters or responds</td>
              </tr>
            </tbody>
          </table>
        </section>
      </div>
    </div>
  )
}
