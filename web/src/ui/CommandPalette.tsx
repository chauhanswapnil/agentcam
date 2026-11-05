import { Command } from 'cmdk'
import { LayoutDashboard, Palette, Search } from 'lucide-react'
import type { ReactNode } from 'react'
import { useLocation } from 'wouter'
import { Kbd } from './Kbd'
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
  const go = (to: string) => () => {
    navigate(to)
    onOpenChange(false)
  }

  const actions: Action[] = [
    { id: 'overview', label: 'Overview', icon: <LayoutDashboard />, group: 'Go to', run: go('/') },
    { id: 'styleguide', label: 'Styleguide', icon: <Palette />, group: 'Go to', run: go('/styleguide') },
    {
      id: 'search',
      label: 'Search sessions, commands and files',
      icon: <Search />,
      group: 'Search',
      hint: 'Coming soon',
      disabled: true,
    },
  ]
  const groups = [...new Set(actions.map((a) => a.group))]

  return (
    <Command.Dialog
      open={open}
      onOpenChange={onOpenChange}
      label="Command palette"
      overlayClassName={s.overlay}
      contentClassName={s.dialog}
      loop
    >
      <div className={s.inputRow}>
        <Search aria-hidden />
        <Command.Input className={s.input} placeholder="Type a command or search…" autoFocus />
        <Kbd keys={['esc']} />
      </div>
      <Command.List className={s.list}>
        <Command.Empty className={s.empty}>Nothing matches.</Command.Empty>
        {groups.map((g) => (
          <Command.Group key={g} heading={g}>
            {actions
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
