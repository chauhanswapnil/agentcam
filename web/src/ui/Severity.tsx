import { ShieldAlert } from 'lucide-react'
import type { Severity } from '../lib/api'
import { SEVERITY_COLOR } from '../lib/severity'
import { Badge } from './Badge'

export function SeverityBadge({ severity, count }: { severity: Severity; count?: number }) {
  const word = severity[0].toUpperCase() + severity.slice(1)
  return (
    <Badge icon={<ShieldAlert />} color={SEVERITY_COLOR[severity]} title={`${word} risk`}>
      {count != null ? `${count} · ${word}` : word}
    </Badge>
  )
}
