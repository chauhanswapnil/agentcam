/** One CSV column: a header and how to read it from a row. */
export type Column<T> = { label: string; value: (row: T) => string | number | boolean | null | undefined }

function cell(v: string | number | boolean | null | undefined): string {
  if (v == null) return ''
  let s = String(v)
  // A spreadsheet runs cells that start with these as formulas.
  if (typeof v === 'string' && /^[=+\-@\t\r]/.test(s)) s = `'${s}`
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s
}

export function toCsv<T>(columns: Column<T>[], rows: T[]): string {
  const lines = [columns.map((c) => cell(c.label)).join(',')]
  for (const r of rows) lines.push(columns.map((c) => cell(c.value(r))).join(','))
  return lines.join('\r\n') + '\r\n'
}

/** Save `rows` as a CSV file, built in the browser from what the page shows. */
export function downloadCsv<T>(filename: string, columns: Column<T>[], rows: T[]) {
  const blob = new Blob([toCsv(columns, rows)], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  window.setTimeout(() => URL.revokeObjectURL(url), 1000)
}
