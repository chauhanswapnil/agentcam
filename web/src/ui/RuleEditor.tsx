import { Check, ListChecks, Save } from 'lucide-react'
import { useState } from 'react'
import { sendJson, useApi, type RulesInfo } from '../lib/api'
import { SEVERITY_COLOR } from '../lib/severity'
import { Button } from './Button'
import { Panel } from './Panel'
import s from './RuleEditor.module.css'

const EXAMPLE = `# Your own rules. Same id as a built-in rule replaces it.
# Fields: id, title, severity (low|medium|high|critical), pattern (regex),
# field (title|body|input|path|any), categories, kinds, actions, secret.
- id: deploy
  title: Deployed to production
  severity: high
  categories: [command]
  pattern: '\\b(fly deploy|vercel --prod|kubectl apply)\\b'
`

/** See every risk rule and edit your own in YAML. */
export function RuleEditor() {
  const [rev, setRev] = useState(0)
  const { data } = useApi<RulesInfo>('/api/rules', rev)
  const [draft, setDraft] = useState<string | null>(null)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [showAll, setShowAll] = useState(false)
  if (!data) return null
  const text = draft ?? (data.user_yaml || EXAMPLE)
  const builtin = new Set(data.builtin_ids)
  const userIds = new Set(data.rules.filter((r) => !builtin.has(r.id) || data.user_yaml.includes(`id: ${r.id}`)).map((r) => r.id))

  const send = async (dry: boolean) => {
    try {
      const r = await sendJson<{ rules: number }>('POST', '/api/rules', { yaml: text, dry_run: dry })
      setMsg({ ok: true, text: dry ? `Looks good: ${r.rules} rules in all.` : `Saved. Re-checking every session with ${r.rules} rules…` })
      if (!dry) {
        setDraft(null)
        setRev((n) => n + 1)
      }
    } catch (e) {
      setMsg({ ok: false, text: (e as Error).message })
    }
  }

  return (
    <Panel title="Rules" icon={<ListChecks />}>
      <div className={s.wrap}>
        <p className={s.intro}>
          {data.rules.length} rules run on every step at index time. Write your own below; they're saved to{' '}
          <code>{data.user_path}</code>.
        </p>
        <textarea
          className={s.editor}
          value={text}
          onChange={(e) => {
            setDraft(e.target.value)
            setMsg(null)
          }}
          spellCheck={false}
          aria-label="Your rules (YAML)"
          rows={Math.min(24, Math.max(10, text.split('\n').length + 1))}
        />
        <div className={s.actions}>
          {msg && <span className={msg.ok ? s.ok : s.error}>{msg.text}</span>}
          <span className={s.spacer} />
          <Button size="sm" onClick={() => send(true)}>
            <Check aria-hidden /> Check
          </Button>
          <Button size="sm" variant="primary" onClick={() => send(false)}>
            <Save aria-hidden /> Save and re-check
          </Button>
        </div>
        <button type="button" className={s.toggle} onClick={() => setShowAll(!showAll)}>
          {showAll ? '▾' : '▸'} All {data.rules.length} rules
        </button>
        {showAll && (
          <div className={s.tableWrap}>
            <table className={s.table}>
              <tbody>
                {data.rules.map((r) => (
                  <tr key={r.id}>
                    <td>
                      <i className={s.dot} style={{ background: SEVERITY_COLOR[r.severity] }} />
                      {r.severity}
                    </td>
                    <td className={s.title}>
                      {r.title}
                      {userIds.has(r.id) && <span className={s.yours}>yours</span>}
                    </td>
                    <td className={s.mono}>{r.outside_project ? 'writes outside the project' : `${r.field}: ${r.pattern}`}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </Panel>
  )
}
