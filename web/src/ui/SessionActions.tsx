import { Check, Download, SquareArrowOutUpRight, SquareTerminal } from 'lucide-react'
import { useState } from 'react'
import type { AgentId } from '../lib/agents'
import { EDITORS, exportUrl, resumeCommand, setEditor, useEditor, type EditorId } from '../lib/session'
import { Button } from './Button'
import { Tooltip } from './Tooltip'
import s from './SessionActions.module.css'

/** Copy the command that resumes a session in its own folder. */
export function ResumeButton({
  agent,
  id,
  cwd,
  compact,
}: {
  agent: AgentId
  id: string
  cwd: string | null
  /** Icon only, for list rows. */
  compact?: boolean
}) {
  const [copied, setCopied] = useState(false)
  const cmd = resumeCommand(agent, id, cwd)
  const copy = async (e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      await navigator.clipboard.writeText(cmd)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      window.prompt('Copy this command', cmd)
    }
  }
  const icon = copied ? <Check aria-hidden /> : <SquareTerminal aria-hidden />
  return (
    <Tooltip content={copied ? 'Copied' : cmd}>
      {compact ? (
        <Button icon size="sm" variant="ghost" aria-label="Copy resume command" onClick={copy}>
          {icon}
        </Button>
      ) : (
        <Button size="sm" onClick={copy}>
          {icon} {copied ? 'Copied' : 'Copy resume command'}
        </Button>
      )}
    </Tooltip>
  )
}

/** Download the session as Markdown or JSON. */
export function ExportLinks({ id }: { id: string }) {
  return (
    <span className={s.group}>
      <a className={s.link} href={exportUrl(id, 'md')} download>
        <Download aria-hidden /> Markdown
      </a>
      <a className={s.link} href={exportUrl(id, 'json')} download>
        JSON
      </a>
    </span>
  )
}

/** Open a file at a line in the editor picked below. */
export function OpenInEditor({ path, line = 1 }: { path: string; line?: number }) {
  const ed = useEditor()
  if (!path.startsWith('/')) return null
  return (
    <Tooltip content={`Open in ${ed.label}`}>
      <a className={s.open} href={ed.url(path, line)} aria-label={`Open ${path} in ${ed.label}`} onClick={(e) => e.stopPropagation()}>
        <SquareArrowOutUpRight aria-hidden />
      </a>
    </Tooltip>
  )
}

/** Which editor "open" links use; remembered in this browser. */
export function EditorPicker() {
  const ed = useEditor()
  return (
    <select className={s.select} value={ed.id} onChange={(e) => setEditor(e.target.value as EditorId)} aria-label="Editor for open links">
      {EDITORS.map((e) => (
        <option key={e.id} value={e.id}>
          {e.label}
        </option>
      ))}
    </select>
  )
}
