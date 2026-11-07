import { StrictMode, Suspense } from 'react'
import { createRoot } from 'react-dom/client'
import './styles/base.css'
import App from './App.tsx'
import { LazyTeamApp } from './team/LazyTeamApp.tsx'

// The team server serves this same build; its pages live under /team.
const team = window.location.pathname === '/team' || window.location.pathname.startsWith('/team/')

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {team ? (
      <Suspense fallback={null}>
        <LazyTeamApp />
      </Suspense>
    ) : (
      <App />
    )}
  </StrictMode>,
)
