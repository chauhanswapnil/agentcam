import { Route, Switch } from 'wouter'
import { AppShell } from './AppShell'
import { Home } from './pages/Home'
import { Styleguide } from './pages/Styleguide'

export default function App() {
  return (
    <AppShell>
      <Switch>
        <Route path="/" component={Home} />
        <Route path="/styleguide" component={Styleguide} />
        <Route>
          <Home />
        </Route>
      </Switch>
    </AppShell>
  )
}
