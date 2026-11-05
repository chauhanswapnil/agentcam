/** Mirrors `Agent` in agentcam-core (kebab-case on the wire). */
export type AgentId = 'claude-code' | 'codex'

export const AGENTS: Record<AgentId, { label: string; short: string; color: string }> = {
  'claude-code': { label: 'Claude Code', short: 'CC', color: 'var(--agent-claude-code)' },
  codex: { label: 'Codex', short: 'CX', color: 'var(--agent-codex)' },
}
