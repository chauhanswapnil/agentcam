/** Words for the setup flags in agentcam-core `fleet.rs`. Neutral: worth a look, not a verdict. */
export const FLAG_LABEL: Record<string, { label: string; note: string; good?: boolean }> = {
  approvals_skipped_by_default: {
    label: 'Approvals skipped by default',
    note: 'Tool calls run without asking unless a rule says otherwise (bypass mode, or Codex approval_policy = never).',
  },
  sandbox_off: { label: 'Sandbox off', note: 'Commands run with full access to the machine.' },
  broad_allow_rule: { label: 'Allow rule covers every shell command', note: 'An allow rule like Bash(*) lets any command run without asking.' },
  danger_prompt_skipped: { label: 'Bypass-mode warning turned off', note: 'The prompt before entering bypass mode is skipped.' },
  guard_hook_installed: { label: 'agentcam guard hook installed', note: 'Guard rules are checked before each tool call.', good: true },
}

export const flagLabel = (id: string) => FLAG_LABEL[id]?.label ?? id.replace(/_/g, ' ')

export const ITEM_KIND: Record<string, string> = {
  mcp_server: 'MCP servers',
  plugin: 'Plugins',
  skill: 'Skills',
  hook: 'Hooks',
}

export type Inventory = {
  os: string
  arch: string
  agentcam_version: string
  agents: { agent: string; version: string | null; sessions: number; last_used: string | null }[]
  items: { kind: string; agent: string; name: string; detail: string | null; scope: string }[]
  flags: { id: string; agent: string; scope: string }[]
}
