import { Check, EyeOff, Undo2 } from 'lucide-react'
import { useState } from 'react'
import { sendJson } from '../lib/api'
import { baseName } from '../lib/format'
import s from './FindingReview.module.css'

type Props = {
  sessionId: string
  seq: number
  ruleId: string
  /** `reviewed`, `muted` or none. */
  review: string | null | undefined
  project: string | null
  onDone: () => void
}

/**
 * Mark a finding as looked at, or mute its rule for this project; undo
 * either. Marks live in `reviews.json` next to the index.
 */
export function FindingReview({ sessionId, seq, ruleId, review, project, onDone }: Props) {
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)
  const send = (body: object) => async (e: React.MouseEvent) => {
    e.stopPropagation()
    setBusy(true)
    setErr(null)
    try {
      await sendJson('POST', '/api/reviews', body)
      onDone()
    } catch (x) {
      setErr((x as Error).message)
    } finally {
      setBusy(false)
    }
  }
  const finding = { session_id: sessionId, seq, rule_id: ruleId }
  if (review === 'reviewed') {
    return (
      <span className={s.row}>
        <span className={s.state}>
          <Check aria-hidden /> Looked at
        </span>
        <button type="button" className={s.action} disabled={busy} onClick={send({ action: 'unreview', ...finding })}>
          <Undo2 aria-hidden /> Undo
        </button>
      </span>
    )
  }
  if (review === 'muted') {
    return (
      <span className={s.row}>
        <span className={s.state}>
          <EyeOff aria-hidden /> Rule muted{project ? ` in ${baseName(project)}` : ''}
        </span>
        <button type="button" className={s.action} disabled={busy} onClick={send({ action: 'unmute', rule_id: ruleId, project })}>
          <Undo2 aria-hidden /> Unmute
        </button>
      </span>
    )
  }
  return (
    <span className={s.row}>
      <button type="button" className={s.action} disabled={busy} onClick={send({ action: 'review', ...finding, note: null })}>
        <Check aria-hidden /> Looks fine
      </button>
      {project && (
        <button
          type="button"
          className={s.action}
          disabled={busy}
          title={`Hide this rule's findings in ${baseName(project)}. Undo any time.`}
          onClick={send({ action: 'mute', rule_id: ruleId, project, note: null })}
        >
          <EyeOff aria-hidden /> Mute in {baseName(project)}
        </button>
      )}
      {err && <span className={s.err}>{err}</span>}
    </span>
  )
}
