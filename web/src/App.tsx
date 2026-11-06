import { Route, Switch } from 'wouter'
import { AppShell } from './AppShell'
import { Audit } from './pages/Audit'
import { Commits } from './pages/Commits'
import { Files } from './pages/Files'
import { Overview } from './pages/Overview'
import { Replay } from './pages/Replay'
import { Search } from './pages/Search'
import { Styleguide } from './pages/Styleguide'
import { Tools } from './pages/Tools'

export default function App() {
  return (
    <AppShell>
      <Switch>
        <Route path="/" component={Overview} />
        <Route path="/sessions/*" component={Replay} />
        <Route path="/search" component={Search} />
        <Route path="/audit" component={Audit} />
        <Route path="/tools" component={Tools} />
        <Route path="/files" component={Files} />
        <Route path="/commits" component={Commits} />
        <Route path="/styleguide" component={Styleguide} />
        <Route>
          <Overview />
        </Route>
      </Switch>
    </AppShell>
  )
}
