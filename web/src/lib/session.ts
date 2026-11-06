import { useSyncExternalStore } from 'react'
import type { AgentId } from './agents'

const quote = (s: string) => (/^[\w/.+@:-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`)

/**
 * The command that picks a session up again, run in the folder it ran in
 * (worktrees included). Subagents resume through their parent. Mirrors
 * `export::resume_command` in agentcam-core.
 */
export function resumeCommand(agent: AgentId, id: string, cwd: string | null): string {
  const sid = id.split('/')[0]
  const cmd = agent === 'codex' ? `codex resume ${sid}` : `claude --resume ${sid}`
  return cwd ? `cd ${quote(cwd)} && ${cmd}` : cmd
}

export const exportUrl = (id: string, format: 'md' | 'json') => `/api/sessions/${id}/export?format=${format}`

export type EditorId = 'vscode' | 'cursor' | 'zed'

/** Editors that open files from a URL. */
export const EDITORS: { id: EditorId; label: string; url: (path: string, line: number) => string }[] = [
  { id: 'vscode', label: 'VS Code', url: (p, l) => `vscode://file${encodeURI(p)}:${l}` },
  { id: 'cursor', label: 'Cursor', url: (p, l) => `cursor://file${encodeURI(p)}:${l}` },
  { id: 'zed', label: 'Zed', url: (p, l) => `zed://file${encodeURI(p)}:${l}` },
]

const KEY = 'agentcam.editor'
function load(): EditorId {
  try {
    const v = window.localStorage.getItem(KEY)
    return EDITORS.some((e) => e.id === v) ? (v as EditorId) : 'vscode'
  } catch {
    return 'vscode'
  }
}
let editor: EditorId = load()
const listeners = new Set<() => void>()

export function setEditor(id: EditorId) {
  editor = id
  try {
    window.localStorage.setItem(KEY, id)
  } catch {
    // Storage blocked: the choice lasts until reload.
  }
  listeners.forEach((l) => l())
}

export function useEditor() {
  const id = useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => editor,
  )
  return EDITORS.find((e) => e.id === id) ?? EDITORS[0]
}

/** First changed line of a diff (the `+c` in `@@ -a,b +c,d @@`), else 1. */
export function firstLine(diff: string | null | undefined): number {
  const m = diff ? /^@@ -\d+(?:,\d+)? \+(\d+)/m.exec(diff) : null
  return m ? Math.max(1, Number(m[1])) : 1
}
