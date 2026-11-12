import { X } from 'lucide-react'
import { useEffect, useRef, type ReactNode } from 'react'
import { Button } from './Button'
import s from './Drawer.module.css'

type Props = {
  title: ReactNode
  /** Plain text for screen readers; the dialog's name. */
  label: string
  onClose: () => void
  children: ReactNode
}

/**
 * A panel on the right edge that holds a list or a detail without leaving
 * the page. Escape and a click on the backdrop close it; focus moves in on
 * open and goes back to what opened it on close.
 */
export function Drawer({ title, label, onClose, children }: Props) {
  const drawerRef = useRef<HTMLElement>(null)

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
    drawerRef.current?.querySelector<HTMLElement>('[data-close]')?.focus()
    return () => opener?.focus?.()
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <div className={s.overlay} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <aside ref={drawerRef} className={s.drawer} role="dialog" aria-modal="true" aria-label={label}>
        <header className={s.head}>
          <h2>{title}</h2>
          <Button icon variant="ghost" aria-label="Close" onClick={onClose} data-close>
            <X aria-hidden />
          </Button>
        </header>
        <div className={s.body}>{children}</div>
      </aside>
    </div>
  )
}
