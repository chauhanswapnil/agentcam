import { Ban, CircleCheck, X } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Link } from 'wouter'
import { getJson, query, sendJson, useApi, useDebounced, type GuardInfo, type GuardTest } from '../lib/api'
import { Button } from './Button'
import s from './BlockDialog.module.css'

type Props = {
  sessionId: string
  seq: number
  tool: string
  title: string
  why?: string
  onClose: () => void
}

/** Turn a step into a guard rule that the Claude Code hook enforces. */
export function BlockDialog({ sessionId, seq, tool: initialTool, title, why, onClose }: Props) {
  const [tool, setTool] = useState(initialTool)
  const [pattern, setPattern] = useState('')
  const [reason, setReason] = useState(
    `${why ? `${why}. ` : ''}Blocked by an agentcam rule: ask the user before doing this.`,
  )
  const [error, setError] = useState('')
  const [saved, setSaved] = useState(false)
  const info = useApi<GuardInfo>('/api/guard', saved ? 1 : 0)

  useEffect(() => {
    getJson<{ tool: string; pattern: string }>(`/api/guard/suggest${query({ tool: initialTool, title })}`)
      .then((r) => {
        setTool(r.tool)
        setPattern(r.pattern)
      })
      .catch(() => {})
  }, [initialTool, title])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const p = useDebounced(pattern, 250)
  const test = useApi<GuardTest>(p ? `/api/guard/test${query({ tool: tool || '*', pattern: p })}` : null)

  const save = async () => {
    setError('')
    try {
      await sendJson('POST', '/api/guard', { tool, pattern, reason, from_session: sessionId, from_seq: seq })
      setSaved(true)
    } catch (e) {
      setError((e as Error).message)
    }
  }

  return (
    <div className={s.overlay} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={s.dialog} role="dialog" aria-modal="true" aria-label="Block next time">
        <div className={s.head}>
          <h2>Block next time</h2>
          <Button icon variant="ghost" aria-label="Close" onClick={onClose}>
            <X aria-hidden />
          </Button>
        </div>
        {saved ? (
          <div className={s.done}>
            <p>
              <CircleCheck aria-hidden style={{ width: 16, verticalAlign: -3, color: 'var(--status-good)' }} /> Saved. The
              next time your agent tries a matching call, it is stopped and told why.
            </p>
            {info.data && info.data.hooks.every((h) => !h.installed) && (
              <p>
                One more step: the hook isn't in your agent settings yet.{' '}
                <Link href="/audit#guard" onClick={onClose} style={{ textDecoration: 'underline' }}>
                  Set it up on the Audit page
                </Link>
                .
              </p>
            )}
            <div className={s.actions}>
              <Button variant="primary" onClick={onClose}>
                Done
              </Button>
            </div>
          </div>
        ) : (
          <>
            <p className={s.intro}>
              A Claude Code or Codex hook runs <code>agentcam guard</code> before every tool call and stops the ones that
              match.
              Nothing changes for calls that don't match.
            </p>
            <label className={s.field}>
              Tool (hook name such as Bash, Edit, Write, apply_patch; or * for any)
              <input className={`${s.input} ${s.mono}`} value={tool} onChange={(e) => setTool(e.target.value)} />
            </label>
            <label className={s.field}>
              Pattern (regular expression, checked against the command, file path or URL)
              <input className={`${s.input} ${s.mono}`} value={pattern} onChange={(e) => setPattern(e.target.value)} />
            </label>
            <label className={s.field}>
              Message for the agent
              <input className={s.input} value={reason} onChange={(e) => setReason(e.target.value)} />
            </label>
            <div className={s.test} aria-live="polite">
              {!test.data ? (
                'Checking past sessions…'
              ) : !test.data.valid ? (
                <span className={s.error}>Pattern error: {test.data.error}</span>
              ) : (
                <>
                  Would have blocked <b style={{ color: 'var(--text-1)' }}>{test.data.matches}</b> past{' '}
                  {test.data.matches === 1 ? 'step' : 'steps'}.
                  {test.data.examples.length > 0 && (
                    <ul>
                      {test.data.examples.map((x) => (
                        <li key={`${x.session_id}:${x.seq}`}>{x.title}</li>
                      ))}
                    </ul>
                  )}
                </>
              )}
            </div>
            {error && <p className={s.error}>{error}</p>}
            <div className={s.actions}>
              <Button variant="ghost" onClick={onClose}>
                Cancel
              </Button>
              <Button variant="primary" onClick={save} disabled={!pattern || test.data?.valid === false}>
                <Ban aria-hidden /> Block next time
              </Button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
