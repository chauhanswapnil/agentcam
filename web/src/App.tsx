import { Route, Switch } from 'wouter'
import { AppShell } from './AppShell'
import { Overview } from './pages/Overview'
import { Session } from './pages/Session'
import { Styleguide } from './pages/Styleguide'

export default function App() {
  return (
    <AppShell>
      <Switch>
        <Route path="/" component={Overview} />
        <Route path="/sessions/*" component={Session} />
        <Route path="/styleguide" component={Styleguide} />
        <Route>
          <Overview />
        </Route>
      </Switch>
    </AppShell>
  )
}
