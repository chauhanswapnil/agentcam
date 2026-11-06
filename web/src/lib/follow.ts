import type { FollowRule } from './api'

/** A rule in plain words. */
export function ruleText(r: FollowRule): string {
  switch (r.kind) {
    case 'checks':
      return `Run ${(r.commands ?? []).map((c) => `\`${c}\``).join(', ')} after the last edit${r.when ? ` to \`${r.when}/\`` : ''}`
    case 'package_manager':
      return `Use ${r.tool} for packages and scripts`
    case 'dont_edit':
      return `Don't edit \`${r.path}\``
    case 'dont_run':
      return `Don't run \`${r.command}\``
  }
}
