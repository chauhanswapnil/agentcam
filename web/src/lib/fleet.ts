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
  outside_content_open_shell: {
    label: 'Outside content can reach a shell without asking',
    note: 'An MCP server that brings in issues, messages or pages, and shell commands that run without a question.',
  },
  mcp_secret_in_config: { label: 'Secret written in an MCP config', note: 'A token in plain text in an MCP server\'s env, headers or arguments.' },
  mcp_plain_http: { label: 'MCP server over plain http', note: 'A remote MCP server reached without TLS.' },
  mcp_flagged_text: { label: 'MCP tool text aimed at the agent', note: 'A tool description or server instructions with hidden text or orders for the agent.' },
  skill_flagged: { label: 'Skill worth a look', note: 'A skill whose text or scripts raised a check (hidden text, secrets, a download piped to a shell).' },
  instructions_flagged: { label: 'Instruction file worth a look', note: 'An AGENTS.md or CLAUDE.md with hidden characters or text aimed at the agent.' },
  hook_missing_program: { label: 'Hook pointing at a missing file', note: 'A hook command names a script that is not on disk.' },
}

/** Words for tags on MCP servers and skills (`fleet.rs` `Item.tags`). */
export const TAG_LABEL: Record<string, string> = {
  outside_content: 'outside content',
  unpinned: 'latest release',
  secret_in_config: 'secret in config',
  plain_http: 'plain http',
  flagged_text: 'tool text worth a look',
  description_changed: 'description changed',
  flagged: 'worth a look',
}

/** Tags that call for a second look, shown with a warning dot. */
export const TAG_LOOK = new Set(['secret_in_config', 'plain_http', 'flagged_text', 'flagged'])

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
  items: { kind: string; agent: string; name: string; detail: string | null; scope: string; tags?: string[] }[]
  flags: { id: string; agent: string; scope: string }[]
}
