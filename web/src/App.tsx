import { lazy, Suspense } from 'react'
import { Link, Route, Switch } from 'wouter'
import { AppShell } from './AppShell'
import { Overview } from './pages/Overview'
import { Replay } from './pages/Replay'
import { EmptyState } from './ui/Misc'

// Overview and the replay load with the app; other pages load on first visit.
const page = <K extends string>(load: () => Promise<Record<K, React.ComponentType>>, name: K) =>
  lazy(() => load().then((m) => ({ default: m[name] })))

const Live = page(() => import('./pages/Live'), 'Live')
const Profile = page(() => import('./pages/Profile'), 'Profile')
const Lasted = page(() => import('./pages/Lasted'), 'Lasted')
const Compare = page(() => import('./pages/Compare'), 'Compare')
const Skills = page(() => import('./pages/Skills'), 'Skills')
const Instructions = page(() => import('./pages/Instructions'), 'Instructions')
const Search = page(() => import('./pages/Search'), 'Search')
const Thinking = page(() => import('./pages/Thinking'), 'Thinking')
const Audit = page(() => import('./pages/Audit'), 'Audit')
const Tools = page(() => import('./pages/Tools'), 'Tools')
const Files = page(() => import('./pages/Files'), 'Files')
const Commits = page(() => import('./pages/Commits'), 'Commits')
const Outbox = page(() => import('./pages/Outbox'), 'Outbox')
const BlamePage = page(() => import('./pages/Blame'), 'BlamePage')
const Prompts = page(() => import('./pages/Prompts'), 'Prompts')
const Setup = page(() => import('./pages/Setup'), 'Setup')
const Styleguide = page(() => import('./pages/Styleguide'), 'Styleguide')

export default function App() {
  return (
    <AppShell>
      <Suspense fallback={null}>
        <Switch>
          <Route path="/" component={Overview} />
          <Route path="/sessions/*" component={Replay} />
          <Route path="/live" component={Live} />
          <Route path="/profile" component={Profile} />
          <Route path="/lasted" component={Lasted} />
          <Route path="/compare" component={Compare} />
          <Route path="/skills" component={Skills} />
          <Route path="/instructions" component={Instructions} />
          <Route path="/search" component={Search} />
          <Route path="/thinking" component={Thinking} />
          <Route path="/audit" component={Audit} />
          <Route path="/tools" component={Tools} />
          <Route path="/files" component={Files} />
          <Route path="/commits" component={Commits} />
          <Route path="/outbox" component={Outbox} />
          <Route path="/blame" component={BlamePage} />
          <Route path="/prompts" component={Prompts} />
          <Route path="/setup" component={Setup} />
          <Route path="/styleguide" component={Styleguide} />
          <Route>
            <EmptyState title="No page here">
              <Link href="/">Back to the overview</Link>
            </EmptyState>
          </Route>
        </Switch>
      </Suspense>
    </AppShell>
  )
}
