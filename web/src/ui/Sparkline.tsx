/** Tiny bar chart of a short series, newest on the right. Decorative:
 * the numbers it summarizes are always shown beside it. */
export function Sparkline({ values, width = 72, height = 20 }: { values: number[]; width?: number; height?: number }) {
  const max = Math.max(1, ...values)
  const gap = 2
  const w = (width - gap * (values.length - 1)) / values.length
  return (
    <svg width={width} height={height} aria-hidden style={{ display: 'block', flex: 'none' }}>
      {values.map((v, i) => {
        const h = v > 0 ? Math.max(2, (v / max) * height) : 1
        return (
          <rect
            key={i}
            x={i * (w + gap)}
            y={height - h}
            width={w}
            height={h}
            rx={1}
            fill={v > 0 ? 'var(--heat-3)' : 'var(--line-2)'}
          />
        )
      })}
    </svg>
  )
}
