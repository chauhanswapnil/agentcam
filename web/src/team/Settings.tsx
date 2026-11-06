import { History, KeyRound, Settings2, UserPlus } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { formatAgo } from '../lib/format'
import { Button } from '../ui/Button'
import { Skeleton } from '../ui/Misc'
import { Panel } from '../ui/Panel'
import v from '../pages/Views.module.css'
import { teamPost, useTeamApi, type Settings } from './api'
import s from './Team.module.css'

export function TeamSettings({ role }: { role: 'admin' | 'viewer' }) {
  const [gen, setGen] = useState(0)
  const { data } = useTeamApi<Settings>('/api/team/settings', gen)
  const admin = role === 'admin'
  return (
    <div className={v.page}>
      <header className={v.head}>
        <div>
          <h1>Settings</h1>
          <p>
            {admin
              ? 'Teams, invites, viewers and what the views show. Every change here is logged.'
              : 'Only admins change settings.'}
          </p>
        </div>
      </header>
      {!data ? (
        <Skeleton height={240} />
      ) : (
        <>
          <General key={`${data.org_name}|${data.seat_price_usd}|${data.show_people}`} data={data} admin={admin} onSaved={() => setGen(gen + 1)} />
          {admin && <Invites data={data} onMade={() => setGen(gen + 1)} />}
          {admin && <Viewers onMade={() => setGen(gen + 1)} />}
          {admin && (
            <Panel title="Audit log" icon={<History />} flush>
              <div className={v.tableWrap}>
                <table className={v.table}>
                  <thead>
                    <tr>
                      <th>When</th>
                      <th>Who</th>
                      <th>What</th>
                      <th>Detail</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.log.map((l, i) => (
                      <tr key={i}>
                        <td>{formatAgo(l.ts)}</td>
                        <td>{l.actor}</td>
                        <td>{l.action}</td>
                        <td>{l.detail ?? ''}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Panel>
          )}
        </>
      )}
    </div>
  )
}

function General({ data, admin, onSaved }: { data: Settings; admin: boolean; onSaved: () => void }) {
  const [org, setOrg] = useState(data.org_name)
  const [price, setPrice] = useState(data.seat_price_usd == null ? '' : String(data.seat_price_usd))
  const [people, setPeople] = useState(data.show_people)
  const [msg, setMsg] = useState<string | null>(null)
  const save = async (e: FormEvent) => {
    e.preventDefault()
    try {
      await teamPost('/api/team/settings', {
        org_name: org,
        seat_price_usd: price === '' ? undefined : Number(price),
        show_people: people,
      })
      setMsg('Saved')
      onSaved()
    } catch (err) {
      setMsg((err as Error).message)
    }
  }
  return (
    <Panel title="General" icon={<Settings2 />}>
      <form className={s.form} onSubmit={save}>
        <label className={s.field}>
          <span className={s.label}>Organization name</span>
          <input className={v.filter} value={org} onChange={(e) => setOrg(e.target.value)} disabled={!admin} />
        </label>
        <label className={s.field}>
          <span className={s.label}>Seat price per month (USD)</span>
          <input
            className={v.filter}
            inputMode="decimal"
            value={price}
            onChange={(e) => setPrice(e.target.value.replace(/[^0-9.]/g, ''))}
            placeholder="e.g. 150"
            disabled={!admin}
          />
          <span className={s.hint}>For flat-rate plans. The Value page compares it with use at API prices.</span>
        </label>
        <label className={s.check}>
          <input type="checkbox" checked={people} onChange={(e) => setPeople(e.target.checked)} disabled={!admin} />
          <span>
            Show people's names
            <span className={s.hint}>
              Off by default: machines show as short ids. Views never rank people either way.
            </span>
          </span>
        </label>
        {admin && (
          <div className={s.formRow}>
            <Button variant="primary" type="submit">
              Save
            </Button>
            {msg && <span className={s.hint}>{msg}</span>}
          </div>
        )}
      </form>
    </Panel>
  )
}

function Invites({ data, onMade }: { data: Settings; onMade: () => void }) {
  const [team, setTeam] = useState(data.teams[0]?.name ?? '')
  const [made, setMade] = useState<{ code: string; team: string; expires_in_days: number } | null>(null)
  const [err, setErr] = useState<string | null>(null)
  const make = async (e: FormEvent) => {
    e.preventDefault()
    try {
      setMade(await teamPost('/api/team/invites', { team }))
      setErr(null)
      onMade()
    } catch (x) {
      setErr((x as Error).message)
    }
  }
  const server = window.location.origin
  return (
    <Panel title="Invite machines" icon={<UserPlus />}>
      <form className={s.form} onSubmit={make}>
        <label className={s.field}>
          <span className={s.label}>Team (new names make a new team)</span>
          <input className={v.filter} list="teams" value={team} onChange={(e) => setTeam(e.target.value)} placeholder="Platform" />
          <datalist id="teams">
            {data.teams.map((t) => (
              <option key={t.id} value={t.name} />
            ))}
          </datalist>
        </label>
        <div className={s.formRow}>
          <Button variant="primary" type="submit" disabled={!team.trim()}>
            Make invite
          </Button>
          {err && <span className={s.hint}>{err}</span>}
        </div>
      </form>
      {made && (
        <div className={s.made}>
          <p className={v.note}>
            Send this to people on {made.team}. It works for 50 machines for {made.expires_in_days} days, and is shown
            only once.
          </p>
          <pre className={s.code}>agentcam team join {server} {made.code}</pre>
        </div>
      )}
      {data.teams.length > 0 && (
        <p className={v.note}>
          Teams: {data.teams.map((t) => `${t.name} (${t.machines} machine${t.machines === 1 ? '' : 's'})`).join(', ')}
        </p>
      )}
    </Panel>
  )
}

function Viewers({ onMade }: { onMade: () => void }) {
  const [name, setName] = useState('')
  const [role, setRole] = useState<'viewer' | 'admin'>('viewer')
  const [token, setToken] = useState<string | null>(null)
  const make = async (e: FormEvent) => {
    e.preventDefault()
    const r = await teamPost<{ token: string }>('/api/team/viewers', { name, role })
    setToken(r.token)
    setName('')
    onMade()
  }
  return (
    <Panel title="Viewer tokens" icon={<KeyRound />}>
      <form className={s.form} onSubmit={make}>
        <label className={s.field}>
          <span className={s.label}>Who it's for</span>
          <input className={v.filter} value={name} onChange={(e) => setName(e.target.value)} placeholder="Eng leads" />
        </label>
        <label className={s.field}>
          <span className={s.label}>Role</span>
          <select className={v.select} value={role} onChange={(e) => setRole(e.target.value as 'viewer' | 'admin')}>
            <option value="viewer">Viewer: sees every page</option>
            <option value="admin">Admin: also changes settings</option>
          </select>
        </label>
        <div className={s.formRow}>
          <Button variant="primary" type="submit" disabled={!name.trim()}>
            Make token
          </Button>
        </div>
      </form>
      {token && (
        <div className={s.made}>
          <p className={v.note}>Shown once. They sign in with this link:</p>
          <pre className={s.code}>
            {window.location.origin}/team/login#{token}
          </pre>
        </div>
      )}
    </Panel>
  )
}
