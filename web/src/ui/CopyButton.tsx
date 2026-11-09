import { Check, Copy } from 'lucide-react'
import { useEffect, useState } from 'react'
import s from './CopyButton.module.css'

/**
 * Copy a piece of text, with a short "Copied" note. Falls back to a prompt
 * the person can copy from when the clipboard is blocked.
 */
export function CopyButton({ text, label = 'Copy', className }: { text: string; label?: string; className?: string }) {
  const [done, setDone] = useState(false)
  useEffect(() => {
    if (!done) return
    const t = window.setTimeout(() => setDone(false), 1400)
    return () => window.clearTimeout(t)
  }, [done])
  const copy = async (e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      await navigator.clipboard.writeText(text)
      setDone(true)
    } catch {
      window.prompt('Copy this', text)
    }
  }
  return (
    <button type="button" className={`${s.copy} ${className ?? ''}`} onClick={copy} aria-label={label} title={label}>
      {done ? <Check aria-hidden /> : <Copy aria-hidden />}
      <span className={s.text}>{done ? 'Copied' : label}</span>
    </button>
  )
}
