import { Download } from 'lucide-react'
import { downloadCsv, type Column } from '../lib/csv'
import { localDay } from '../lib/format'
import { Button } from './Button'

/** Download the rows a table shows as CSV. */
export function CsvButton<T>({ name, columns, rows }: { name: string; columns: Column<T>[]; rows: T[] }) {
  return (
    <Button
      size="sm"
      variant="ghost"
      disabled={rows.length === 0}
      onClick={() => downloadCsv(`agentcam-${name}-${localDay(new Date())}.csv`, columns, rows)}
      title={`Download these ${rows.length} rows as CSV`}
    >
      <Download aria-hidden /> CSV
    </Button>
  )
}
