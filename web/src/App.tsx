import { Route, Switch } from 'wouter'
import { AppShell } from './AppShell'
import { Overview } from './pages/Overview'
import { Replay } from './pages/Replay'
import { Search } from './pages/Search'
import { Styleguide } from './pages/Styleguide'

export default function App() {
  return (
    <AppShell>
      <Switch>
        <Route path="/" component={Overview} />
        <Route path="/sessions/*" component={Replay} />
        <Route path="/search" component={Search} />
        <Route path="/styleguide" component={Styleguide} />
        <Route>
          <Overview />
        </Route>
      </Switch>
    </AppShell>
  )
}
