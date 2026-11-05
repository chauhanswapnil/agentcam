import s from './Kbd.module.css'

const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform)

const GLYPHS: Record<string, string> = {
  mod: isMac ? '⌘' : 'Ctrl',
  shift: '⇧',
  alt: isMac ? '⌥' : 'Alt',
  enter: '↵',
  esc: 'Esc',
  up: '↑',
  down: '↓',
  left: '←',
  right: '→',
  space: 'Space',
}

/** `<Kbd keys={['mod', 'k']} />` renders ⌘ K on a Mac, Ctrl K elsewhere. */
export function Kbd({ keys }: { keys: string[] }) {
  return (
    <kbd className={s.keys}>
      {keys.map((k) => (
        <span key={k} className={s.key}>
          {GLYPHS[k] ?? k.toUpperCase()}
        </span>
      ))}
    </kbd>
  )
}
