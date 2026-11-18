import { useEffect, useState } from 'react'
import { api, type LayoutExample, type LibraryKind } from '../../api/client'
import styles from './Libraries.module.css'

interface Props {
  kind: LibraryKind
  template: string
  /** Read-only folders are never renamed, so only the item folder is shown. */
  readOnly: boolean
}

/** Sample files placed by the layout, so you see what a template does as you type it. */
export function LayoutExamples({ kind, template, readOnly }: Props) {
  const [examples, setExamples] = useState<LayoutExample[] | null>(null)

  useEffect(() => {
    let current = true
    const timer = window.setTimeout(() => {
      api.checkTemplate(template, kind).then(
        (result) => {
          // An invalid template keeps the last examples; its error shows at the field.
          if (current && result.valid) {
            setExamples(result.examples)
          }
        },
        () => undefined,
      )
    }, 250)
    return () => {
      current = false
      window.clearTimeout(timer)
    }
  }, [template, kind])

  if (!examples || examples.length === 0) {
    return null
  }
  return (
    <div className={styles.examples}>
      <p className={styles.field}>{readOnly ? 'For example, found in' : 'For example'}</p>
      <ul>
        {examples.map((example) => (
          <li key={example.filename}>
            <span className={styles.exampleFrom}>{example.filename}</span>
            <span aria-hidden="true">→</span>
            <Placed example={example} readOnly={readOnly} />
          </li>
        ))}
      </ul>
    </div>
  )
}

function Placed({ example, readOnly }: { example: LayoutExample; readOnly: boolean }) {
  const folder = example.folder ?? ''
  if (readOnly && folder) {
    return <code className={styles.item}>{folder}/</code>
  }
  if (example.path) {
    const inside = example.path.startsWith(`${folder}/`) ? example.path.slice(folder.length) : ''
    return (
      <code>
        <span className={styles.item}>{inside ? folder : example.path}</span>
        {inside && <span className={styles.below}>{inside}</span>}
      </code>
    )
  }
  const token = example.problem?.match(/^No value for (\{\w+\})/)?.[1]
  return (
    <span className={styles.exampleProblem}>
      {token
        ? `Organise skips it: there’s no ${token} for this file`
        : `Organise skips it: ${example.problem?.replace(/\.$/, '') ?? 'the layout can’t name it'}`}
    </span>
  )
}
