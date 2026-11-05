import { Command } from 'cmdk'
import { LayoutDashboard, Palette, Search } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { useLocation } from 'wouter'
import { AGENTS } from '../lib/agents'
import { query, useApi, useDebounced, type Hit } from '../lib/api'
import { baseName, oneLine } from '../lib/format'
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
  disabled?: boolean
  run?: () => void
}

export function CommandPalette({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const [, navigate] = useLocation()
  const [text, setText] = useState('')
  const q = useDebounced(text.trim(), 150)
  const hits = useApi<Hit[]>(q.length >= 2 ? `/api/search${query({ q, limit: 8 })}` : null)
  const go = (to: string) => () => {
    navigate(to)
    onOpenChange(false)
    setText('')
  }

  const actions: Action[] = [
    { id: 'overview', label: 'Overview', icon: <LayoutDashboard />, group: 'Go to', run: go('/') },
    { id: 'styleguide', label: 'Styleguide', icon: <Palette />, group: 'Go to', run: go('/styleguide') },
    { id: 'search', label: 'Search page', icon: <Search />, group: 'Go to', run: go('/search') },
  ]
  // The server ranks search hits, so filter the static actions ourselves.
  const needle = text.trim().toLowerCase()
  const shown = actions.filter((a) => !needle || a.label.toLowerCase().includes(needle))
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
          placeholder="Search prompts, commands, files…"
          value={text}
          onValueChange={setText}
          autoFocus
        />
        <Kbd keys={['esc']} />
      </div>
      <Command.List className={s.list}>
        {!shown.length && !found.length && (
          <div className={s.empty}>{hits.loading ? 'Searching…' : 'Nothing matches.'}</div>
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
        {groups.map((g) => (
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
