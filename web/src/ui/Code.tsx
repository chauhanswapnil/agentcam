import { Fragment } from 'react'

/** Text with `backticked` spans shown as code. */
export function CodeText({ text }: { text: string }) {
  return (
    <>
      {text.split('`').map((part, i) =>
        i % 2 ? (
          <code key={i} style={{ fontFamily: 'var(--font-mono)', fontSize: '0.92em' }}>
            {part}
          </code>
        ) : (
          <Fragment key={i}>{part}</Fragment>
        ),
      )}
    </>
  )
}
