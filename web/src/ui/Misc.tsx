import { createElement, type CSSProperties, type ReactNode } from 'react'
import { AGENTS, type AgentId } from '../lib/agents'
import { GROUPS, groupOf, iconFor, kindLabel, type EventKind, type ToolCategory } from '../lib/kinds'
import s from './Misc.module.css'

export function Skeleton({ width = '100%', height = 12, style }: { width?: number | string; height?: number; style?: CSSProperties }) {
  return <span className={s.skeleton} style={{ width, height, ...style }} aria-hidden />
}

export function EmptyState({ icon, title, children }: { icon?: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className={s.empty}>
      {icon}
      <p className={s.emptyTitle}>{title}</p>
      {children && <p>{children}</p>}
    </div>
  )
}

/** Agent mark and name. The color always sits beside the name. */
export function AgentLabel({ agent, hideName }: { agent: AgentId; hideName?: boolean }) {
  const a = AGENTS[agent]
  return (
    <span className={s.agent} title={a.label}>
      <span className={s.agentMark} style={{ background: a.color }} aria-hidden>
        {a.short}
      </span>
      {hideName ? <span className="sr-only">{a.label}</span> : a.label}
    </span>
  )
}

/** Icon tile for one step: group color behind, exact-kind icon on top. */
export function KindIcon({
  kind,
  category,
  isError,
}: {
  kind: EventKind
  category?: ToolCategory | null
  isError?: boolean
}) {
  const g = GROUPS[groupOf(kind, category, isError)]
  return (
    <span
      className={s.kind}
      style={{ color: g.color, background: `color-mix(in oklab, ${g.color} 16%, transparent)` }}
      title={kindLabel(kind, category)}
    >
      {createElement(iconFor(kind, category), { 'aria-hidden': true })}
      <span className="sr-only">{kindLabel(kind, category)}</span>
    </span>
  )
}
