import s from './DiffView.module.css'

type Line = { kind: 'add' | 'del' | 'ctx' | 'hunk' | 'meta'; text: string; old?: number; new?: number }

/** Parse unified diff text into numbered lines. */
function parse(diff: string): Line[] {
  const out: Line[] = []
  let o = 0
  let n = 0
  for (const raw of diff.replace(/\n$/, '').split('\n')) {
    const m = raw.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/)
    if (m) {
      o = Number(m[1])
      n = Number(m[2])
      out.push({ kind: 'hunk', text: raw })
    } else if (raw.startsWith('+++') || raw.startsWith('---') || raw.startsWith('diff ') || raw.startsWith('index ')) {
      out.push({ kind: 'meta', text: raw })
    } else if (raw.startsWith('+')) {
      out.push({ kind: 'add', text: raw.slice(1), new: n++ })
    } else if (raw.startsWith('-')) {
      out.push({ kind: 'del', text: raw.slice(1), old: o++ })
    } else {
      out.push({ kind: 'ctx', text: raw.startsWith(' ') ? raw.slice(1) : raw, old: o++, new: n++ })
    }
  }
  return out
}

/** Whole new file content, shown as all-added lines. */
function asCreate(content: string): Line[] {
  return content
    .replace(/\n$/, '')
    .split('\n')
    .map((text, i) => ({ kind: 'add', text, new: i + 1 }))
}

const SIGN = { add: '+', del: '−', ctx: ' ', hunk: '', meta: '' }

export function DiffView({ diff, created }: { diff: string; created?: boolean }) {
  const lines = created ? asCreate(diff) : parse(diff)
  return (
    <div className={s.diff}>
      <table className={s.table}>
        <tbody>
          {lines.map((l, i) =>
            l.kind === 'hunk' || l.kind === 'meta' ? (
              <tr key={i} className={`${s.line} ${s[l.kind]}`}>
                <td colSpan={4}>{l.text}</td>
              </tr>
            ) : (
              <tr key={i} className={`${s.line} ${l.kind !== 'ctx' ? s[l.kind] : ''}`}>
                <td className={s.num}>{l.old ?? ''}</td>
                <td className={s.num}>{l.new ?? ''}</td>
                <td className={s.sign}>{SIGN[l.kind]}</td>
                <td>{l.text || ' '}</td>
              </tr>
            ),
          )}
        </tbody>
      </table>
    </div>
  )
}
