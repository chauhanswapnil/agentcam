import { Copy, Download, Image, X } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { drawCard, type CardData } from '../lib/reportCard'
import { Button } from './Button'
import s from './ReportCard.module.css'

type Props = {
  /** Build the card; `redact` hides titles, projects and paths. */
  build: (redact: boolean) => CardData | null
  filename: string
  label?: string
}

/** A button that opens a preview of a shareable PNG card. */
export function ReportCardButton({ build, filename, label = 'Report card' }: Props) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button size="sm" onClick={() => setOpen(true)}>
        <Image aria-hidden /> {label}
      </Button>
      {open && <ReportCardDialog build={build} filename={filename} onClose={() => setOpen(false)} />}
    </>
  )
}

function ReportCardDialog({ build, filename, onClose }: Omit<Props, 'label'> & { onClose: () => void }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  // Names and paths are hidden by default: cards are for sharing.
  const [redact, setRedact] = useState(true)
  const [note, setNote] = useState('')
  const data = build(redact)
  // Redraw only when the card's content changes, not on every parent render.
  const key = JSON.stringify(data)

  useEffect(() => {
    if (canvas.current && key !== 'null') void drawCard(canvas.current, JSON.parse(key) as CardData)
  }, [key])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const blob = () => new Promise<Blob | null>((r) => canvas.current?.toBlob(r, 'image/png'))
  const download = async () => {
    const b = await blob()
    if (!b) return
    const a = document.createElement('a')
    a.href = URL.createObjectURL(b)
    a.download = filename
    a.click()
    URL.revokeObjectURL(a.href)
  }
  const copy = async () => {
    try {
      const b = await blob()
      if (!b) return
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': b })])
      setNote('Copied to the clipboard.')
    } catch {
      setNote("This browser can't copy images. Use Download instead.")
    }
  }

  return (
    <div className={s.overlay} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={s.dialog} role="dialog" aria-modal="true" aria-label="Report card">
        <div className={s.head}>
          <h2>Report card</h2>
          <Button icon variant="ghost" aria-label="Close" onClick={onClose}>
            <X aria-hidden />
          </Button>
        </div>
        {data ? <canvas ref={canvas} className={s.canvas} /> : <p className={s.note}>Nothing to show yet.</p>}
        <div className={s.actions}>
          <label className={s.check}>
            <input type="checkbox" checked={redact} onChange={(e) => setRedact(e.target.checked)} />
            Hide titles, projects and file names
          </label>
          <span className={s.spacer} />
          <Button size="sm" onClick={copy} disabled={!data}>
            <Copy aria-hidden /> Copy
          </Button>
          <Button size="sm" variant="primary" onClick={download} disabled={!data}>
            <Download aria-hidden /> Download PNG
          </Button>
        </div>
        {note && <p className={s.note}>{note}</p>}
        <p className={s.note}>Made in your browser. Nothing is uploaded; secrets are already masked.</p>
      </div>
    </div>
  )
}
