import type { ReactNode } from 'react'
import s from './Markdown.module.css'

/**
 * A small Markdown renderer for agent replies: headings, paragraphs, lists,
 * code, quotes, tables, bold, italics, inline code and links. It builds
 * React elements only (never HTML strings), and links are shown, not
 * followed, so nothing in a log can load or run anything.
 */
export function Markdown({ text, className }: { text: string; className?: string }) {
  return <div className={`${s.md} ${className ?? ''}`}>{blocks(text)}</div>
}

function blocks(text: string): ReactNode[] {
  const lines = text.replace(/\r\n/g, '\n').split('\n')
  const out: ReactNode[] = []
  let i = 0
  const key = () => out.length
  while (i < lines.length) {
    const line = lines[i]
    if (!line.trim()) {
      i++
      continue
    }
    const fence = line.match(/^\s*(```|~~~)/)
    if (fence) {
      const body: string[] = []
      i++
      while (i < lines.length && !lines[i].trim().startsWith(fence[1])) body.push(lines[i++])
      i++
      out.push(
        <pre key={key()}>
          <code>{body.join('\n')}</code>
        </pre>,
      )
      continue
    }
    const h = line.match(/^\s{0,3}(#{1,4})\s+(.*)$/)
    if (h) {
      const Tag = `h${Math.min(h[1].length + 1, 4)}` as 'h2' | 'h3' | 'h4'
      out.push(<Tag key={key()}>{inline(h[2])}</Tag>)
      i++
      continue
    }
    if (/^\s*>/.test(line)) {
      const body: string[] = []
      while (i < lines.length && /^\s*>/.test(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ''))
      out.push(<blockquote key={key()}>{inline(body.join(' '))}</blockquote>)
      continue
    }
    if (/^\s*\|.*\|\s*$/.test(line) && i + 1 < lines.length && /^\s*\|[\s:|-]+\|\s*$/.test(lines[i + 1])) {
      const row = (l: string) => l.trim().replace(/^\||\|$/g, '').split('|').map((c) => c.trim())
      const head = row(line)
      i += 2
      const rows: string[][] = []
      while (i < lines.length && /^\s*\|.*\|\s*$/.test(lines[i])) rows.push(row(lines[i++]))
      out.push(
        <table key={key()}>
          <thead>
            <tr>
              {head.map((c, j) => (
                <th key={j}>{inline(c)}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r, ri) => (
              <tr key={ri}>
                {r.map((c, j) => (
                  <td key={j}>{inline(c)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>,
      )
      continue
    }
    const list = line.match(/^\s*([-*+]|\d+[.)])\s+/)
    if (list) {
      const ordered = /\d/.test(list[1])
      const items: string[] = []
      while (i < lines.length) {
        const m = lines[i].match(/^\s*([-*+]|\d+[.)])\s+(.*)$/)
        if (m) items.push(m[2])
        else if (lines[i].trim() && /^\s{2,}/.test(lines[i]) && items.length) items[items.length - 1] += ' ' + lines[i].trim()
        else break
        i++
      }
      const L = ordered ? 'ol' : 'ul'
      out.push(
        <L key={key()}>
          {items.map((it, j) => (
            <li key={j}>{inline(it)}</li>
          ))}
        </L>,
      )
      continue
    }
    const para: string[] = []
    while (
      i < lines.length &&
      lines[i].trim() &&
      !/^\s*(```|~~~|#{1,4}\s|>|([-*+]|\d+[.)])\s)/.test(lines[i])
    )
      para.push(lines[i++])
    // A line no rule above took (say `    ## x`, indented too far for a
    // heading) still has to move us on, or this loop never ends.
    if (!para.length) para.push(lines[i++])
    out.push(<p key={key()}>{inline(para.join('\n'))}</p>)
  }
  return out
}

/** Inline code, bold, italics and links. */
function inline(text: string): ReactNode[] {
  const out: ReactNode[] = []
  const re = /(`[^`]+`)|(\*\*[^*]+\*\*)|(\[[^\]]+\]\([^)\s]+\))|(\*[^*\s][^*]*\*)/g
  let last = 0
  let m: RegExpExecArray | null
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index))
    const t = m[0]
    const k = out.length
    if (m[1]) out.push(<code key={k}>{t.slice(1, -1)}</code>)
    else if (m[2]) out.push(<strong key={k}>{inline(t.slice(2, -2))}</strong>)
    else if (m[3]) {
      const [, label, href] = t.match(/^\[([^\]]+)\]\(([^)]+)\)$/)!
      out.push(
        <span key={k} className={s.link} title={href}>
          {label}
        </span>,
      )
    } else out.push(<em key={k}>{t.slice(1, -1)}</em>)
    last = m.index + t.length
  }
  if (last < text.length) out.push(text.slice(last))
  return out
}
