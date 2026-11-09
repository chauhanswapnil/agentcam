import { Check, ListChecks, Save, ScanSearch } from 'lucide-react'
import { Link } from 'wouter'
import { useState } from 'react'
import { sendJson, useApi, type RulesInfo } from '../lib/api'
import { SEVERITY_COLOR } from '../lib/severity'
import { Button } from './Button'
import { Panel } from './Panel'
import s from './RuleEditor.module.css'

const EXAMPLE = `# Your own rules. Same id as a built-in rule replaces it.
# Fields: id, title, severity (low|medium|high|critical), pattern (regex),
# field (title|body|input|path|any), categories, kinds, actions, secret,
# unless (regex that cancels a match), tags (for sequences).
- id: deploy
  title: Deployed to production
  severity: high
  categories: [command]
  tags: [deploy]
  pattern: '\\b(fly deploy|vercel --prod|kubectl apply)\\b'
  unless: '--dry-run'

# A sequence: steps in order within a window, matched by tag or rule id.
# Every step also has context tags: outside-content, guardrails-off,
# denied, command, edit, web, mcp.
- id: deploy-after-secret
  title: Deployed after reading a secret
  severity: high
  sequence:
    - tags: [secret-read]
    - tags: [deploy]
  within: 20
`

/** See every risk rule and edit your own in YAML. */
export function RuleEditor() {
  const [rev, setRev] = useState(0)
  const { data } = useApi<RulesInfo>('/api/rules', rev)
  const [draft, setDraft] = useState<string | null>(null)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [showAll, setShowAll] = useState(false)
  const [preview, setPreview] = useState<Preview | null>(null)
  const [previewing, setPreviewing] = useState(false)
  if (!data) return null
  const text = draft ?? (data.user_yaml || EXAMPLE)
  const builtin = new Set(data.builtin_ids)
  const userIds = new Set(data.rules.filter((r) => !builtin.has(r.id) || data.user_yaml.includes(`id: ${r.id}`)).map((r) => r.id))

  const runPreview = async () => {
    setPreviewing(true)
    setMsg(null)
    try {
      setPreview(await sendJson<Preview>('POST', '/api/rules/preview', { yaml: text }))
    } catch (e) {
      setPreview(null)
      setMsg({ ok: false, text: (e as Error).message })
    } finally {
      setPreviewing(false)
    }
  }

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
            setPreview(null)
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
          <Button size="sm" onClick={runPreview} disabled={previewing}>
            <ScanSearch aria-hidden /> {previewing ? 'Looking…' : 'Preview matches'}
          </Button>
          <Button size="sm" variant="primary" onClick={() => send(false)}>
            <Save aria-hidden /> Save and re-check
          </Button>
        </div>
        {preview && (
          <div className={s.preview}>
            <p className={s.previewNote}>
              In the last {preview.days} days ({preview.sessions_checked} most recent sessions), before saving:
            </p>
            <ul className={s.previewList}>
              {preview.rules.map((r) => (
                <li key={r.rule_id}>
                  <span className={s.previewRule}>
                    {r.title} <span className={s.mono}>{r.rule_id}</span>
                  </span>
                  <span className={s.previewCount}>
                    {r.matches === 0
                      ? 'No matches'
                      : `${r.matches} ${r.matches === 1 ? 'match' : 'matches'} in ${r.sessions} ${r.sessions === 1 ? 'session' : 'sessions'}`}
                  </span>
                  {r.examples.map((x) => (
                    <Link key={`${x.session_id}:${x.seq}`} href={`/sessions/${x.session_id}?step=${x.seq}`} className={s.previewExample}>
                      <code>{x.step ?? x.excerpt}</code> · {x.session_title ?? 'Untitled session'}
                    </Link>
                  ))}
                </li>
              ))}
            </ul>
          </div>
        )}
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
                    <td className={s.mono}>{describe(r)}</td>
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

type Preview = {
  sessions_checked: number
  days: number
  rules: { rule_id: string; title: string; matches: number; sessions: number; examples: { session_id: string; seq: number; excerpt: string; step: string | null; session_title: string | null }[] }[]
}

/** What a rule checks, in one line. */
function describe(r: RulesInfo['rules'][number]): string {
  if (r.outside_project) return 'writes outside the project'
  if (r.sequence?.length) {
    const links = r.sequence.map((l) => [...(l.tags ?? []), ...(l.rules ?? [])].join(' or ')).join(' → ')
    return `${links} within ${r.within ?? 30} steps`
  }
  if (r.outside_content?.length) return `commands using a package or link from ${r.outside_content.join(', ')} content`
  const on = r.when_tagged?.length ? `${r.when_tagged.join(', ')} · ` : ''
  return `${on}${r.field}: ${r.pattern}${r.unless ? `  unless ${r.unless}` : ''}`
}
