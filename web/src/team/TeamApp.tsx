import { KeyRound, LogOut } from 'lucide-react'
import { useEffect, useState, type FormEvent } from 'react'
import { Link, Route, Switch, useLocation } from 'wouter'
import { Button } from '../ui/Button'
import { Logo } from '../ui/Logo'
import { Panel } from '../ui/Panel'
import v from '../pages/Views.module.css'
import shell from '../AppShell.module.css'
import { getToken, setToken, useTeamApi, type Me } from './api'
import { Fleet } from './Fleet'
import { Governance } from './Governance'
import { Insights } from './Insights'
import { TeamSettings } from './Settings'
import { Value } from './Value'
import s from './Team.module.css'

const NAV = [
  { href: '/team', label: 'Insights' },
  { href: '/team/value', label: 'Value' },
  { href: '/team/governance', label: 'Governance' },
  { href: '/team/fleet', label: 'Fleet' },
  { href: '/team/settings', label: 'Settings' },
]

/** The team server's UI. Same design system as the local app, its own pages. */
export function TeamApp() {
  const [location] = useLocation()
  if (location === '/team/login') return <Login />
  if (!getToken()) return <Login />
  return <Shell />
}

function Shell() {
  const [location, navigate] = useLocation()
  const me = useTeamApi<Me>('/api/team/me')
  useEffect(() => {
    if (me.auth) {
      setToken(null)
      navigate('/team/login')
    }
  }, [me.auth, navigate])
  return (
    <div className={shell.shell}>
      <header className={shell.top}>
        <Link href="/team" aria-label="agentcam team home">
          <Logo />
        </Link>
        <nav className={shell.nav}>
          {NAV.map((n) => (
            <Link key={n.href} href={n.href} className={`${shell.navLink} ${location === n.href ? shell.navActive : ''}`}>
              {n.label}
            </Link>
          ))}
        </nav>
        <div className={shell.spacer} />
        {me.data && (
          <span className={s.who} title={`Signed in as ${me.data.name} (${me.data.role})`}>
            {me.data.org}
          </span>
        )}
        <Button
          variant="ghost"
          size="sm"
          icon
          aria-label="Sign out"
          title="Sign out"
          onClick={() => {
            setToken(null)
            navigate('/team/login')
          }}
        >
          <LogOut />
        </Button>
      </header>
      <main className={shell.main}>
        <Switch>
          <Route path="/team/value" component={Value} />
          <Route path="/team/governance" component={Governance} />
          <Route path="/team/fleet" component={Fleet} />
          <Route path="/team/settings">
            <TeamSettings role={me.data?.role ?? 'viewer'} />
          </Route>
          <Route>
            <Insights />
          </Route>
        </Switch>
      </main>
    </div>
  )
}

function Login() {
  const [, navigate] = useLocation()
  const [token, setT] = useState('')
  // `/team/login#ac_view_…` from `agentcam team serve` signs in at once,
  // and the token leaves the address bar.
  useEffect(() => {
    const take = () => {
      const h = window.location.hash.slice(1)
      if (h.startsWith('ac_view_')) {
        setToken(h)
        window.history.replaceState(null, '', '/team')
        navigate('/team')
      }
    }
    take()
    window.addEventListener('hashchange', take)
    return () => window.removeEventListener('hashchange', take)
  }, [navigate])
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!token.trim()) return
    setToken(token.trim())
    navigate('/team')
  }
  return (
    <div className={s.login}>
      <Logo />
      <Panel title="Sign in to the team server" icon={<KeyRound />}>
        <form onSubmit={submit} className={s.loginForm}>
          <label htmlFor="token" className={s.label}>
            Viewer token
          </label>
          <input
            id="token"
            className={v.filter}
            style={{ width: '100%' }}
            value={token}
            onChange={(e) => setT(e.target.value)}
            placeholder="ac_view_…"
            autoComplete="off"
            spellCheck={false}
          />
          <p className={v.note}>
            The first admin token prints when the server starts. Make more with{' '}
            <code>agentcam team token --role viewer</code>. It's kept in this browser only.
          </p>
          <Button variant="primary" type="submit">
            Sign in
          </Button>
        </form>
      </Panel>
    </div>
  )
}
