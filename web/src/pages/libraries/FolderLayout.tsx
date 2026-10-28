import { useRef, useState } from 'react'
import type { LibraryKind, WritePolicy } from '../../api/client'
import { DEFAULT_ABOVE, joinTemplate, splitTemplate } from './layout'
import { TemplateEditor } from './TemplateEditor'
import { Legend } from './Legend'
import styles from './Libraries.module.css'

interface Props {
  id: string
  kind: LibraryKind
  policy: WritePolicy
  /** The whole template. Below the item folder is only edited for managed folders. */
  value: string
  onChange: (value: string, valid: boolean) => void
}

/**
 * Where each series (or movie) folder sits: at the top of the folder, or under
 * grouping levels you spell out. Inside it the matcher finds its own way; only
 * a managed folder needs to say how files are arranged there.
 */
export function FolderLayout({ id, kind, policy, value, onChange }: Props) {
  const { above, below } = splitTemplate(value, kind)
  const fallback = DEFAULT_ABOVE[kind]
  const [custom, setCustom] = useState(above !== fallback)
  const item = kind === 'movies' ? 'movie' : 'series'
  // Each side's last check; the template is valid when both are.
  const valid = useRef({ above: true, below: true })
  const change = (part: 'above' | 'below', next: string, ok: boolean) => {
    valid.current[part] = ok
    const template = part === 'above' ? joinTemplate(next, below) : joinTemplate(above, next)
    onChange(template, valid.current.above && valid.current.below)
  }

  const toggle = (on: boolean) => {
    setCustom(on)
    if (!on) {
      change('above', fallback, true)
    }
  }

  return (
    <div className={styles.block}>
      <p className={styles.muted}>
        {custom ? (
          `Each ${item} folder sits at the path below.`
        ) : (
          <>
            Each {item} has its own folder at the top, named like{' '}
            <code className={styles.item}>{fallback}</code>.
          </>
        )}{' '}
        Inside it, seasons and episodes are recognised however they’re named.
      </p>
      <label className={styles.check}>
        <input type="checkbox" checked={custom} onChange={(e) => toggle(e.target.checked)} />
        Custom format above the {item} folder
      </label>
      {custom && (
        <>
          <TemplateEditor
            id={id}
            kind={kind}
            part="above"
            value={above}
            rest={below}
            onChange={(next, ok) => change('above', next, ok)}
          />
          <Legend kind={kind} part="above" />
        </>
      )}
      {policy === 'managed' && (
        <>
          <label className={styles.field} htmlFor={`${id}-below`}>
            Inside each {item} folder (managed: files are renamed and moved to match)
          </label>
          <TemplateEditor
            id={`${id}-below`}
            kind={kind}
            part="below"
            value={below}
            rest={above}
            onChange={(next, ok) => change('below', next, ok)}
          />
          <Legend kind={kind} part="below" />
        </>
      )}
    </div>
  )
}
