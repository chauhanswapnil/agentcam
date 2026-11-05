import {
  Bot,
  Brain,
  FilePen,
  FilePlus2,
  FileText,
  Globe,
  MessageSquare,
  Plug,
  Search,
  SquareTerminal,
  TriangleAlert,
  User,
  Wrench,
  type LucideIcon,
} from 'lucide-react'

/** Mirrors `EventKind` in agentcam-core. */
export type EventKind = 'prompt' | 'response' | 'thinking' | 'tool_call' | 'system'

/** Mirrors `ToolCategory` in agentcam-core. */
export type ToolCategory =
  | 'command'
  | 'read'
  | 'edit'
  | 'write'
  | 'search'
  | 'web'
  | 'mcp'
  | 'subagent'
  | 'other'

/**
 * The five color groups every step falls into. Color shows the group;
 * the icon shows the exact kind. Never add a group: three hues is the
 * most that stay apart for color-blind readers.
 */
export type KindGroup = 'you' | 'change' | 'look' | 'agent' | 'error'

export const GROUPS: Record<KindGroup, { label: string; color: string; hint: string }> = {
  you: { label: 'You', color: 'var(--kind-you)', hint: 'Prompts you typed' },
  change: { label: 'Changes', color: 'var(--kind-change)', hint: 'Commands, edits and new files' },
  look: { label: 'Looks', color: 'var(--kind-look)', hint: 'Reads, searches, web and MCP calls' },
  agent: { label: 'Agent', color: 'var(--kind-agent)', hint: 'Agent replies and thinking' },
  error: { label: 'Error', color: 'var(--status-critical)', hint: 'Failed steps' },
}

export const GROUP_ORDER: KindGroup[] = ['you', 'change', 'look', 'agent', 'error']

export function groupOf(kind: EventKind, category?: ToolCategory | null, isError = false): KindGroup {
  if (isError) return 'error'
  switch (kind) {
    case 'prompt':
      return 'you'
    case 'response':
    case 'thinking':
      return 'agent'
    case 'system':
      return 'agent'
    case 'tool_call':
      return category === 'command' || category === 'edit' || category === 'write' ? 'change' : 'look'
  }
}

export function iconFor(kind: EventKind, category?: ToolCategory | null): LucideIcon {
  switch (kind) {
    case 'prompt':
      return User
    case 'response':
      return MessageSquare
    case 'thinking':
      return Brain
    case 'system':
      return TriangleAlert
    case 'tool_call':
      break
  }
  switch (category) {
    case 'command':
      return SquareTerminal
    case 'read':
      return FileText
    case 'edit':
      return FilePen
    case 'write':
      return FilePlus2
    case 'search':
      return Search
    case 'web':
      return Globe
    case 'mcp':
      return Plug
    case 'subagent':
      return Bot
    default:
      return Wrench
  }
}

export function kindLabel(kind: EventKind, category?: ToolCategory | null): string {
  if (kind !== 'tool_call') {
    return { prompt: 'Prompt', response: 'Response', thinking: 'Thinking', system: 'Notice' }[kind]
  }
  const labels: Record<ToolCategory, string> = {
    command: 'Command',
    read: 'Read',
    edit: 'Edit',
    write: 'Write',
    search: 'Search',
    web: 'Web',
    mcp: 'MCP',
    subagent: 'Subagent',
    other: 'Tool',
  }
  return labels[category ?? 'other']
}
