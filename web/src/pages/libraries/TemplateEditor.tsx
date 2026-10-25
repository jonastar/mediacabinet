import { useEffect, useRef, useState } from 'react'
import { api, type LibraryKind, type TemplateCheck } from '../../api/client'
import { TemplateText } from './TemplateText'
import styles from './Libraries.module.css'

interface Props {
  id: string
  kind: LibraryKind
  value: string
  onChange: (value: string, valid: boolean) => void
}

/** A template input checked by the server as you type, with a coloured preview. */
export function TemplateEditor({ id, kind, value, onChange }: Props) {
  const [check, setCheck] = useState<TemplateCheck | null>(null)
  const onChangeRef = useRef(onChange)
  useEffect(() => {
    onChangeRef.current = onChange
  })

  useEffect(() => {
    let current = true
    const timer = window.setTimeout(() => {
      api.checkTemplate(value, kind).then(
        (result) => {
          if (current) {
            setCheck(result)
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
  }, [value, kind])

  const invalid = check !== null && !check.valid
  return (
    <div className={styles.block}>
      <input
        id={id}
        className={styles.input}
        value={value}
        spellCheck={false}
        aria-invalid={invalid}
        aria-describedby={`${id}-check`}
        onChange={(e) => onChange(e.target.value, false)}
      />
      <div id={`${id}-check`} aria-live="polite">
        {invalid && <p className={styles.error}>{check.error}</p>}
        {check?.valid && <TemplateText pieces={check.pieces} />}
      </div>
    </div>
  )
}
