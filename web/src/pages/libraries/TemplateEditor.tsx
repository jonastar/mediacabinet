import { useEffect, useRef, useState } from 'react'
import { api, type LibraryKind, type TemplateCheck } from '../../api/client'
import { joinTemplate } from './layout'
import styles from './Libraries.module.css'

interface Props {
  id: string
  kind: LibraryKind
  /** Which side of the item folder this edits: up to and including it, or below it. */
  part: 'above' | 'below'
  value: string
  /** The other side, not edited here but checked along with `value`. */
  rest: string
  onChange: (value: string, valid: boolean) => void
}

/**
 * A template input checked by the server as you type, coloured in place: the
 * text is drawn by a backdrop behind the (transparent) input, so there's one
 * field rather than a field and a preview.
 */
export function TemplateEditor({ id, kind, part, value, rest, onChange }: Props) {
  const [check, setCheck] = useState<{ value: string; result: TemplateCheck } | null>(null)
  const backdrop = useRef<HTMLDivElement>(null)
  const onChangeRef = useRef(onChange)
  useEffect(() => {
    onChangeRef.current = onChange
  })

  useEffect(() => {
    let current = true
    const timer = window.setTimeout(() => {
      const template = part === 'above' ? joinTemplate(value, rest) : joinTemplate(rest, value)
      api.checkTemplate(template, kind).then(
        (result) => {
          if (current) {
            setCheck({ value, result })
            onChangeRef.current(value, result.valid)
          }
        },
        () => undefined,
      )
    }, 250)
    return () => {
      current = false
      window.clearTimeout(timer)
    }
  }, [part, value, rest, kind])

  const result = check?.result ?? null
  const invalid = result !== null && !result.valid
  // Coloured once the check has caught up with the text; plain until then.
  const checked = check?.value === value && check.result.valid ? check.result.pieces : []
  const item = checked.findIndex((p) => p.role === 'item')
  // This side's pieces; below, skipping the separator after the item folder.
  const shown = part === 'above' ? checked.slice(0, item + 1) : checked.slice(item + 2)
  const pieces = shown.length > 0 && shown.map((p) => p.text).join('') === value ? shown : null
  const follow = (e: { currentTarget: HTMLInputElement }) => {
    if (backdrop.current) {
      backdrop.current.scrollLeft = e.currentTarget.scrollLeft
    }
  }
  return (
    <div className={styles.block}>
      <div className={styles.templateField}>
        <div ref={backdrop} className={styles.templateBackdrop} aria-hidden="true">
          {pieces
            ? pieces.map((p, i) => (
                <span key={i} className={styles[p.role]}>
                  {p.text}
                </span>
              ))
            : value}
        </div>
        <input
          id={id}
          className={`${styles.input} ${styles.templateInput}`}
          value={value}
          spellCheck={false}
          aria-invalid={invalid}
          aria-describedby={`${id}-check`}
          onChange={(e) => onChange(e.target.value, false)}
          onScroll={follow}
          onSelect={follow}
        />
      </div>
      <div id={`${id}-check`} aria-live="polite">
        {invalid && <p className={styles.error}>{result.error}</p>}
      </div>
    </div>
  )
}
