import { useState } from 'react'
import { api } from '../api/client'
import { useApi } from '../api/useApi'
import { Failed, Loading } from '../components/Status'
import { Detail } from './review/Detail'
import { entries } from './review/entries'
import styles from './review/Review.module.css'

export function ReviewPage() {
  const queue = useApi('review', api.review)
  const [selected, setSelected] = useState<string | null>(null)

  if (queue.error) {
    return <Failed error={queue.error} onRetry={queue.reload} />
  }
  if (!queue.data) {
    return <Loading />
  }
  const list = entries(queue.data)
  const current = list.find((e) => e.key === selected) ?? list[0]

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <div className={styles.heading}>
          <h1>Review queue</h1>
          <p className={styles.lead}>
            Folders and files the matcher wasn’t sure about. Nothing uncertain goes in until you
            decide.
          </p>
        </div>
        {list.length === 0 ? (
          <div className={styles.empty}>
            Nothing to review. Every file is matched, ignored or waiting for a scan.
          </div>
        ) : (
          <div className={styles.columns}>
            <ul aria-label="To review" className={styles.list}>
              {list.map((e) => (
                <li key={e.key}>
                  <button
                    type="button"
                    className={styles.entry}
                    aria-current={e.key === current.key}
                    onClick={() => setSelected(e.key)}
                  >
                    <span className={styles.entryName}>{e.label}</span>
                    <span className={styles.entryMeta}>
                      <span className={styles.chip}>{e.chip}</span>
                      {e.root}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
            {/* Keyed so each entry starts with its own selection and picker state. */}
            <Detail
              key={current.key}
              entry={current}
              onDone={() => {
                setSelected(null)
                queue.reload()
              }}
            />
          </div>
        )}
      </div>
    </div>
  )
}
