import { CircleCheck, CircleDashed, LoaderCircle } from 'lucide-react'
import type { AgentId } from '../lib/agents'
import type { IndexStatus } from '../lib/api'
import { Logo } from './Logo'
import { AgentLabel } from './Misc'
import s from './Welcome.module.css'

const DEMO_URL = 'https://github.com/chauhanswapnil/agentcam#try-it-with-demo-data'

/**
 * What a first run shows before any session is indexed: progress while the
 * first index runs, then which log folders were checked and what to do next.
 */
export function Welcome({ status }: { status: IndexStatus }) {
  const found = status.folders.filter((f) => f.exists)
  if (status.indexing) {
    return (
      <div className={s.box} role="status">
        <LoaderCircle className={s.spin} aria-hidden />
        <h2 className={s.title}>Reading your agent logs…</h2>
        <p className={s.lead}>The first run reads every session on this machine. It takes a few seconds, more with years of logs.</p>
      </div>
    )
  }
  return (
    <div className={s.box}>
      <Logo size={28} wordmark={false} />
      <h2 className={s.title}>{found.length ? 'No sessions in these folders yet' : 'No Claude Code or Codex logs here yet'}</h2>
      <p className={s.lead}>
        agentcam reads the session logs Claude Code and Codex already write on this machine. Nothing is uploaded.
      </p>
      <ul className={s.folders}>
        {status.folders.map((f) => (
          <li key={f.path}>
            {f.exists ? <CircleCheck className={s.ok} aria-hidden /> : <CircleDashed className={s.no} aria-hidden />}
            <AgentLabel agent={f.agent as AgentId} />
            <code className={s.path}>{f.path}</code>
            <span className={s.state}>{f.exists ? 'found' : 'not here yet'}</span>
          </li>
        ))}
      </ul>
      <ol className={s.steps}>
        <li>Run a session with Claude Code or Codex. It shows here within seconds, with no restart.</li>
        <li>
          Logs somewhere else? Start agentcam with <code>CLAUDE_CONFIG_DIR</code> or <code>CODEX_HOME</code> set to that
          folder.
        </li>
        <li>
          Want to look around first?{' '}
          <a href={DEMO_URL} target="_blank" rel="noreferrer">
            Try it with demo data
          </a>
          .
        </li>
      </ol>
      {status.last_error && <p className={s.error}>The last index run stopped: {status.last_error}</p>}
    </div>
  )
}
