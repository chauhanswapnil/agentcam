import { Search } from 'lucide-react'
import type { ReactNode } from 'react'
import { Link, useLocation } from 'wouter'
import { CommandPalette } from './ui/CommandPalette'
import { useCommandPalette } from './ui/useCommandPalette'
import { Kbd } from './ui/Kbd'
import { Logo } from './ui/Logo'
import s from './AppShell.module.css'

export function AppShell({ children }: { children: ReactNode }) {
  const [open, setOpen] = useCommandPalette()
  const [location] = useLocation()
  const nav = [
    { href: '/', label: 'Overview' },
    { href: '/search', label: 'Search' },
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
