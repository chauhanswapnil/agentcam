import type { KeyboardEvent } from 'react'

/**
 * Props that make a table row act like a button: a click, and Enter or Space
 * when the row itself has focus. Pair it with the `rowBtn` class, which has
 * the hover and focus ring.
 */
export function rowKeys(act: () => void) {
  return {
    onClick: act,
    tabIndex: 0,
    onKeyDown: (e: KeyboardEvent) => {
      if (e.target === e.currentTarget && (e.key === 'Enter' || e.key === ' ')) {
        e.preventDefault()
        act()
      }
    },
  }
}
