import { lazy } from 'react'

// The team pages load only on the team server, under /team.
export const LazyTeamApp = lazy(() => import('./TeamApp.tsx').then((m) => ({ default: m.TeamApp })))
