import { ChevronDown, Hourglass, Search } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Link, useLocation } from 'wouter'
import { useApi, useIndexStatus, type LiveSession } from './lib/api'
import { needsYou, useAlertSettings, useAlerts } from './lib/alerts'
import { CommandPalette } from './ui/CommandPalette'
import { useCommandPalette } from './ui/useCommandPalette'
import { Kbd } from './ui/Kbd'
import { Logo } from './ui/Logo'
import s from './AppShell.module.css'

export function AppShell({ children }: { children: ReactNode }) {
  const [open, setOpen] = useCommandPalette()
  const [location, navigate] = useLocation()
  const gen = useIndexStatus().status?.generation ?? 0
  // Refetch when the index changes and every 30s, so states age out.
  const tick = useTick(30_000)
  const live = useApi<LiveSession[]>('/api/live?minutes=15', gen * 1e6 + tick)
  const active = live.data?.filter((x) => x.state === 'working').length ?? 0
  const alerts = useAlertSettings()
  const waiting = needsYou(live.data, alerts)
  useAlerts(live.data, gen, (id) => navigate(`/sessions/${id}`))
  const nav = [
    { href: '/', label: 'Overview' },
    { href: '/live', label: 'Live' },
    { href: '/search', label: 'Search' },
    { href: '/lasted', label: 'Lasted' },
    { href: '/compare', label: 'Compare' },
    { href: '/skills', label: 'Skills' },
    { href: '/audit', label: 'Audit' },
  ]
  const more = [
    { href: '/setup', label: 'Setup' },
    { href: '/prompts', label: 'Prompts' },
    { href: '/instructions', label: 'Instructions' },
    { href: '/thinking', label: 'Thinking' },
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
          <MoreMenu items={more} location={location} />
        </nav>
        <div className={s.spacer} />
        {waiting > 0 && (
          <Link href="/live" className={s.needs} title="Sessions where the agent replied and may be waiting for you">
            <Hourglass aria-hidden />
            {waiting}
            <span className={s.needsText}> waiting for you</span>
          </Link>
        )}
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

/** A number that goes up every `ms`, to refetch on a timer. */
function useTick(ms: number) {
  const [n, setN] = useState(0)
  useEffect(() => {
    const t = window.setInterval(() => setN((x) => x + 1), ms)
    return () => window.clearInterval(t)
  }, [ms])
  return n
}

/** The less used pages, in a small menu so the top bar stays short. */
function MoreMenu({ items, location }: { items: { href: string; label: string }[]; location: string }) {
  const [open, setOpen] = useState(false)
  const box = useRef<HTMLDivElement>(null)
  const current = items.find((i) => i.href === location)
  useEffect(() => {
    if (!open) return
    const close = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent ? e.key === 'Escape' : !box.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', close)
    document.addEventListener('keydown', close)
    return () => {
      document.removeEventListener('mousedown', close)
      document.removeEventListener('keydown', close)
    }
  }, [open])
  return (
    <div className={s.more} ref={box}>
      <button
        type="button"
        className={`${s.navLink} ${s.moreBtn} ${current ? s.navActive : ''}`}
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen(!open)}
      >
        {current?.label ?? 'More'}
        <ChevronDown aria-hidden />
      </button>
      {open && (
        <div className={s.menu} role="menu">
          {items.map((i) => (
            <Link key={i.href} href={i.href} role="menuitem" onClick={() => setOpen(false)} className={`${s.menuItem} ${i.href === location ? s.navActive : ''}`}>
              {i.label}
            </Link>
          ))}
        </div>
      )}
    </div>
  )
}
