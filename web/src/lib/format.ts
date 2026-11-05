const nf = new Intl.NumberFormat('en-US')
const compact = new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 })

export const formatNumber = (n: number) => nf.format(n)
export const formatCompact = (n: number) => compact.format(n)

/** 2400 -> "2.4s", 95_000 -> "1m 35s", 7_380_000 -> "2h 3m". */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`
  const s = Math.round(ms / 1000)
  if (s < 60) return `${(ms / 1000).toFixed(s < 10 ? 1 : 0)}s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m}m ${s % 60}s`
  return `${Math.floor(m / 60)}h ${m % 60}m`
}

/** Last path segment, for showing a project by its folder name. */
export const baseName = (p: string) => p.replace(/[/\\]+$/, '').split(/[/\\]/).pop() ?? p
