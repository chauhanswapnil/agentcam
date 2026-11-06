import { Search } from 'lucide-react'
import type { ReactNode } from 'react'
import { Link, useLocation } from 'wouter'
import { useApi, useIndexStatus, type LiveSession } from './lib/api'
import { CommandPalette } from './ui/CommandPalette'
import { useCommandPalette } from './ui/useCommandPalette'
import { Kbd } from './ui/Kbd'
import { Logo } from './ui/Logo'
import s from './AppShell.module.css'

export function AppShell({ children }: { children: ReactNode }) {
  const [open, setOpen] = useCommandPalette()
  const [location] = useLocation()
  const gen = useIndexStatus().status?.generation ?? 0
  const live = useApi<LiveSession[]>('/api/live?minutes=5', gen)
  const active = live.data?.filter((x) => x.state === 'working').length ?? 0
  const nav = [
    { href: '/', label: 'Overview' },
    { href: '/live', label: 'Live' },
    { href: '/lasted', label: 'Lasted' },
    { href: '/search', label: 'Search' },
    { href: '/audit', label: 'Audit' },
    { href: '/tools', label: 'Tools' },
    { href: '/files', label: 'Files' },
    { href: '/commits', label: 'Commits' },
    { href: '/outbox', label: 'Outbox' },
  ]
  return (
    <div className={s.shell}>
      <header className={s.top}>
        <Link href="/" aria-label="agentcam home">
          <Logo />
        </Link>
        <nav className={s.nav}>
          {nav.map((n) => (
            <Link key={n.href} href={n.href} className={`${s.navLink} ${location === n.href ? s.navActive : ''}`}>
              {n.label}
              {n.href === '/live' && active > 0 && (
                <span className={s.liveDot} title={`${active} working now`} aria-label={`${active} working now`} />
              )}
            </Link>
          ))}
        </nav>
        <div className={s.spacer} />
        <button type="button" className={s.searchButton} onClick={() => setOpen(true)}>
          <Search aria-hidden />
          <span>Search or jump to…</span>
          <Kbd keys={['mod', 'k']} />
        </button>
      </header>
      <main className={s.main}>{children}</main>
      <CommandPalette open={open} onOpenChange={setOpen} />
    </div>
  )
}
