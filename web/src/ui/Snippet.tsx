/** Search snippet with matched words highlighted. Markers come from the
 * server (\u0001 ... \u0002); the text is never parsed as HTML. */
export function Snippet({ text, className }: { text: string; className?: string }) {
  // oxlint-disable-next-line no-control-regex -- the markers are control characters on purpose
  const parts = text.split(/(\u0001[^\u0002]*\u0002)/g)
  return (
    <span className={className}>
      {parts.map((p, i) =>
        p.startsWith('\u0001') ? (
          <mark key={i} style={{ background: 'rgb(57 135 229 / 30%)', color: 'var(--text-1)', borderRadius: 2, padding: '0 1px' }}>
            {p.slice(1, -1)}
          </mark>
        ) : (
          p.replace(/\s+/g, ' ')
        ),
      )}
    </span>
  )
}
