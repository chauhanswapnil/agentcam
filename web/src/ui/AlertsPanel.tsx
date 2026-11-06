import { Bell, BellOff, BellRing } from 'lucide-react'
import { useState } from 'react'
import {
  ALERT_KINDS,
  enableAlerts,
  inQuietHours,
  notifySupported,
  setAlertSettings,
  useAlertSettings,
  type AlertSettings,
} from '../lib/alerts'
import { baseName } from '../lib/format'
import { Button } from './Button'
import { Panel } from './Panel'
import s from './AlertsPanel.module.css'

function money(v: string): number | null {
  const n = Number(v)
  return v.trim() && Number.isFinite(n) && n > 0 ? n : null
}

/** Notification settings. Stored in this browser only; nothing is sent anywhere. */
export function AlertsPanel({ projects }: { projects: string[] }) {
  const st = useAlertSettings()
  const [opened] = useState(() => Date.now())
  const [perm, setPerm] = useState<string>(() => (notifySupported() ? Notification.permission : 'unsupported'))
  const on = st.enabled && perm === 'granted'
  const kinds = (k: keyof AlertSettings['kinds'], v: boolean) => setAlertSettings({ kinds: { ...st.kinds, [k]: v } })
  const roots = [...new Set([...projects, ...st.muted])].filter(Boolean).sort()

  const turnOn = async () => setPerm(await enableAlerts())
  const test = () => {
    try {
      new Notification('Waiting for you', { body: 'This is how agentcam alerts look.', icon: '/favicon.svg', tag: 'agentcam-test' })
    } catch {
      setPerm('unsupported')
    }
  }

  return (
    <Panel
      title="Alerts"
      icon={on ? <BellRing /> : <Bell />}
      actions={
        on ? (
          <>
            <Button size="sm" variant="ghost" onClick={test}>
              Send a test
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setAlertSettings({ enabled: false })}>
              <BellOff aria-hidden /> Turn off
            </Button>
          </>
        ) : null
      }
    >
      {!on ? (
        <div className={s.intro}>
          <p>
            Get a browser notification when a session is waiting for you, is done for now, or has several steps in a row
            error. Settings stay in this browser; nothing leaves your machine. Keep an agentcam tab open for alerts to
            arrive.
          </p>
          {perm === 'unsupported' ? (
            <p className={s.note}>This browser doesn't support notifications.</p>
          ) : perm === 'denied' ? (
            <p className={s.note}>Notifications are blocked for this page. Allow them in the browser's site settings, then reload.</p>
          ) : (
            <Button variant="primary" size="sm" onClick={turnOn}>
              <Bell aria-hidden /> Turn on alerts
            </Button>
          )}
        </div>
      ) : (
        <div className={s.grid}>
          <fieldset className={s.group}>
            <legend>Notify me when</legend>
            {ALERT_KINDS.map((k) => (
              <label key={k.key} className={s.check} title={k.hint}>
                <input type="checkbox" checked={st.kinds[k.key]} onChange={(e) => kinds(k.key, e.target.checked)} />
                {k.label}
              </label>
            ))}
          </fieldset>
          <fieldset className={s.group} disabled={!st.kinds.cost}>
            <legend>Cost limits (API prices)</legend>
            <label className={s.field}>
              <span>One session over</span>
              <span className={s.money}>
                $
                <input
                  type="number"
                  min="0"
                  step="1"
                  inputMode="decimal"
                  placeholder="off"
                  defaultValue={st.sessionCost ?? ''}
                  onChange={(e) => setAlertSettings({ sessionCost: money(e.target.value) })}
                />
              </span>
            </label>
            <label className={s.field}>
              <span>One day over</span>
              <span className={s.money}>
                $
                <input
                  type="number"
                  min="0"
                  step="1"
                  inputMode="decimal"
                  placeholder="off"
                  defaultValue={st.dayCost ?? ''}
                  onChange={(e) => setAlertSettings({ dayCost: money(e.target.value) })}
                />
              </span>
            </label>
          </fieldset>
          <fieldset className={s.group}>
            <legend>Quiet hours</legend>
            <label className={s.check}>
              <input type="checkbox" checked={st.quiet.on} onChange={(e) => setAlertSettings({ quiet: { ...st.quiet, on: e.target.checked } })} />
              No alerts between
            </label>
            <span className={s.times}>
              <input
                type="time"
                aria-label="Quiet from"
                value={st.quiet.from}
                disabled={!st.quiet.on}
                onChange={(e) => setAlertSettings({ quiet: { ...st.quiet, from: e.target.value } })}
              />
              and
              <input
                type="time"
                aria-label="Quiet until"
                value={st.quiet.to}
                disabled={!st.quiet.on}
                onChange={(e) => setAlertSettings({ quiet: { ...st.quiet, to: e.target.value } })}
              />
            </span>
            {inQuietHours(st.quiet, new Date(opened)) && <span className={s.note}>Quiet now.</span>}
          </fieldset>
          <fieldset className={s.group}>
            <legend>Projects</legend>
            {roots.length === 0 ? (
              <span className={s.note}>Projects with live sessions show here so you can mute them.</span>
            ) : (
              roots.map((r) => {
                const muted = st.muted.includes(r)
                return (
                  <label key={r} className={s.check} title={r}>
                    <input
                      type="checkbox"
                      checked={!muted}
                      onChange={() => setAlertSettings({ muted: muted ? st.muted.filter((m) => m !== r) : [...st.muted, r] })}
                    />
                    {baseName(r)}
                    {muted && <span className={s.note}>muted</span>}
                  </label>
                )
              })
            )}
          </fieldset>
        </div>
      )}
    </Panel>
  )
}
