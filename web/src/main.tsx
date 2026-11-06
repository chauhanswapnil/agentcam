import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './styles/base.css'
import App from './App.tsx'
import { TeamApp } from './team/TeamApp.tsx'

// The team server serves this same build; its pages live under /team.
const team = window.location.pathname === '/team' || window.location.pathname.startsWith('/team/')

createRoot(document.getElementById('root')!).render(<StrictMode>{team ? <TeamApp /> : <App />}</StrictMode>)
