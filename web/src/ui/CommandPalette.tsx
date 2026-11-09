import { Command } from 'cmdk'
import {
  Activity,
  Brain,
  FileCheck2,
  FileCode2,
  FileStack,
  GitCommitHorizontal,
  GitCompare,
  History,
  LayoutDashboard,
  ListChecks,
  MessageSquareText,
  Palette,
  Puzzle,
  RefreshCw,
  ScrollText,
  Search,
  Settings2,
  Share2,
  ShieldAlert,
  Sprout,
  Wrench,
} from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { useLocation } from 'wouter'
import { AGENTS } from '../lib/agents'
import { query, useApi, useDebounced, type Hit, type SessionPage } from '../lib/api'
import { baseName, formatAgo, oneLine } from '../lib/format'
import { Kbd } from './Kbd'
import { KindIcon } from './Misc'
import { Snippet } from './Snippet'
import s from './CommandPalette.module.css'

type Action = {
  id: string
  label: string
  icon: ReactNode
  group: string
  hint?: string
  /** Other words people search by. */
  words?: string
  disabled?: boolean
  run?: () => void
}

/** Every page, with the words someone might type to find it. */
const PAGES: { to: string; label: string; icon: ReactNode; words: string }[] = [
  { to: '/', label: 'Overview', icon: <LayoutDashboard />, words: 'home dashboard sessions projects activity heatmap cost' },
  { to: '/live', label: 'Live', icon: <Activity />, words: 'running now working waiting limits usage quota alerts' },
  { to: '/search', label: 'Search', icon: <Search />, words: 'find full text' },
  { to: '/lasted', label: 'What lasted', icon: <Sprout />, words: 'survival kept lines code still' },
  { to: '/compare', label: 'Compare', icon: <GitCompare />, words: 'models versions effort before after' },
  { to: '/skills', label: 'Skills', icon: <Puzzle />, words: 'plugins listing context' },
  { to: '/audit', label: 'Audit', icon: <ShieldAlert />, words: 'risk findings rules guard approvals security evidence siem' },
  { to: '/setup', label: 'Setup', icon: <Settings2 />, words: 'settings config mcp hooks memory claude.md permissions' },
  { to: '/prompts', label: 'Prompts', icon: <MessageSquareText />, words: 'typed repeated history' },
  { to: '/instructions', label: 'Instructions', icon: <ScrollText />, words: 'agents.md claude.md rules followed' },
  { to: '/thinking', label: 'Thinking', icon: <Brain />, words: 'reasoning' },
  { to: '/tools', label: 'Tools', icon: <Wrench />, words: 'tool calls errors mcp' },
  { to: '/files', label: 'Files', icon: <FileStack />, words: 'changed edited history' },
  { to: '/commits', label: 'Commits', icon: <GitCommitHorizontal />, words: 'git pull requests prs' },
  { to: '/outbox', label: 'Outbox', icon: <Share2 />, words: 'team share metadata sync' },
  { to: '/styleguide', label: 'Styleguide', icon: <Palette />, words: 'design tokens components' },
]

/** Text that looks like a file path: `/…`, `~/…` or `dir/file.ext`. */
const looksLikePath = (t: string) => /^(~?\/|\.{0,2}\/)\S+$/.test(t) || /^[\w.-]+\/[\w./-]*\.\w+$/.test(t)

export function CommandPalette({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const [, navigate] = useLocation()
  const [text, setText] = useState('')
  const q = useDebounced(text.trim(), 150)
  const hits = useApi<Hit[]>(q.length >= 2 ? `/api/search${query({ q, limit: 8 })}` : null)
  const recent = useApi<SessionPage>(open ? '/api/sessions?limit=6' : null)
  const go = (to: string) => () => {
    navigate(to)
    onOpenChange(false)
    setText('')
  }

  const download = (url: string) => () => {
    window.location.href = url
    onOpenChange(false)
    setText('')
  }
  const typed = text.trim()
  const actions: Action[] = [
    ...(looksLikePath(typed)
      ? [
          { id: 'blame', label: `Who wrote ${typed}`, icon: <FileCode2 />, group: 'This file', run: go(`/blame${query({ path: typed })}`) },
          { id: 'history', label: `History of ${typed}`, icon: <History />, group: 'This file', run: go(`/files${query({ path: typed })}`) },
        ]
      : []),
    ...PAGES.map((p) => ({ id: p.to, label: p.label, icon: p.icon, group: 'Go to', words: p.words, run: go(p.to) })),
    {
      id: 'reindex',
      label: 'Re-index logs now',
      icon: <RefreshCw />,
      group: 'Actions',
      words: 'refresh update scan',
      run: () => {
        // The status poll in the top bar picks up the run.
        void fetch('/api/index', { method: 'POST' })
        onOpenChange(false)
        setText('')
      },
    },
    { id: 'evidence', label: 'Download evidence for the last 7 days', icon: <FileCheck2 />, group: 'Actions', words: 'audit bundle export soc', run: download('/api/evidence?range=7d') },
    { id: 'findings', label: 'Download findings as OCSF (30 days)', icon: <ListChecks />, group: 'Actions', words: 'siem export splunk ndjson', run: download('/api/findings/export?range=30d&format=ocsf') },
  ]
  // The server ranks search hits, so filter the static actions ourselves.
  const needle = typed.toLowerCase()
  const shown = actions.filter(
    (a) => a.group === 'This file' || !needle || `${a.label} ${a.words ?? ''}`.toLowerCase().includes(needle),
  )
  const sessions = !needle ? (recent.data?.items ?? []) : []
  const groups = [...new Set(shown.map((a) => a.group))]
  const found = q.length >= 2 ? (hits.data ?? []) : []

  return (
    <Command.Dialog
      open={open}
      onOpenChange={onOpenChange}
      label="Command palette"
      overlayClassName={s.overlay}
      contentClassName={s.dialog}
      shouldFilter={false}
      loop
    >
      <div className={s.inputRow}>
        <Search aria-hidden />
        <Command.Input
          className={s.input}
          placeholder="Search steps, go to a page, or paste a file path…"
          value={text}
          onValueChange={setText}
          autoFocus
        />
        <Kbd keys={['esc']} />
      </div>
      <Command.List className={s.list}>
        {!shown.length && !found.length && !sessions.length && (
          <div className={s.empty}>{hits.loading ? 'Searching…' : 'Nothing matches.'}</div>
        )}
        {groups.filter((g) => g === 'This file').map((g) => (
          <Command.Group key={g} heading={g}>
            {shown
              .filter((a) => a.group === g)
              .map((a) => (
                <Command.Item
                  key={a.id}
                  value={a.label}
                  className={s.item}
                  disabled={a.disabled}
                  onSelect={() => a.run?.()}
                >
                  {a.icon}
                  <span className={s.itemLabel}>{a.label}</span>
                  {a.hint && <span className={s.itemHint}>{a.hint}</span>}
                </Command.Item>
              ))}
          </Command.Group>
        ))}
        {sessions.length > 0 && (
          <Command.Group heading="Recent sessions">
            {sessions.map((x) => (
              <Command.Item key={x.id} value={`session:${x.id}`} className={`${s.item} ${s.hit}`} onSelect={go(`/sessions/${x.id}`)}>
                <History />
                <span className={s.hitText}>
                  <span className={s.itemLabel}>{oneLine(x.title ?? x.first_prompt ?? 'Untitled session').slice(0, 80)}</span>
                  <span className={s.itemHint}>
                    {AGENTS[x.agent]?.label} · {x.project_root ? baseName(x.project_root) : '?'}
                    {x.ended_at && ` · ${formatAgo(x.ended_at)}`}
                  </span>
                </span>
              </Command.Item>
            ))}
          </Command.Group>
        )}
        {found.length > 0 && (
          <Command.Group heading="Steps">
            {found.map((h) => (
              <Command.Item
                key={`${h.session_id}:${h.seq}`}
                value={`${h.session_id}:${h.seq}`}
                className={`${s.item} ${s.hit}`}
                onSelect={go(`/sessions/${h.session_id}?step=${h.seq}`)}
              >
                <KindIcon kind={h.kind} category={h.category} isError={h.is_error} />
                <span className={s.hitText}>
                  <Snippet text={h.snippet || h.title} className={s.hitSnippet} />
                  <span className={s.itemHint}>
                    {AGENTS[h.agent]?.label} · {h.project_root ? baseName(h.project_root) : '?'} ·{' '}
                    {oneLine(h.session_title ?? '').slice(0, 60)}
                  </span>
                </span>
              </Command.Item>
            ))}
            <Command.Item value="all-results" className={s.item} onSelect={go(`/search${query({ q })}`)}>
              <Search />
              <span className={s.itemLabel}>See all results for “{q}”</span>
              <Kbd keys={['enter']} />
            </Command.Item>
          </Command.Group>
        )}
        {groups.filter((g) => g !== 'This file').map((g) => (
          <Command.Group key={g} heading={g}>
            {shown
              .filter((a) => a.group === g)
              .map((a) => (
                <Command.Item
                  key={a.id}
                  value={a.label}
                  className={s.item}
                  disabled={a.disabled}
                  onSelect={() => a.run?.()}
                >
                  {a.icon}
                  <span className={s.itemLabel}>{a.label}</span>
                  {a.hint && <span className={s.itemHint}>{a.hint}</span>}
                </Command.Item>
              ))}
          </Command.Group>
        ))}
      </Command.List>
      <div className={s.footer}>
        <span>
          <Kbd keys={['up']} />
          <Kbd keys={['down']} /> move
        </span>
        <span>
          <Kbd keys={['enter']} /> open
        </span>
      </div>
    </Command.Dialog>
  )
}
