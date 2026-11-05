/** The agentcam mark: a lens with a recording dot, plus the wordmark. */
export function Logo({ size = 20, wordmark = true }: { size?: number; wordmark?: boolean }) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
      <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden>
        <rect width="24" height="24" rx="6" fill="var(--bg-3)" />
        <circle cx="12" cy="12" r="5.5" fill="none" stroke="var(--text-1)" strokeWidth="2" />
        <circle cx="18.5" cy="5.5" r="2.5" fill="var(--rec)" />
      </svg>
      {wordmark && (
        <span style={{ fontWeight: 650, fontSize: 'var(--fs-lg)', letterSpacing: '-0.02em' }}>agentcam</span>
      )}
    </span>
  )
}
