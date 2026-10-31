import { useState } from 'react'
import { api } from '../api/client'
import { useApi } from '../api/useApi'
import { Failed, Loading } from '../components/Status'
import { Detail } from './review/Detail'
import { entries, type Entry } from './review/entries'
import { GroupDetail } from './review/GroupDetail'
import { Ignored } from './review/Ignored'
import { useLiveEvents } from '../lib/live'
import styles from './review/Review.module.css'

function EntryButton({
  entry,
  current,
  expanded,
  onSelect,
}: {
  entry: Entry
  current: Entry
  expanded?: boolean
  onSelect: (key: string) => void
}) {
  return (
    <button
      type="button"
      className={`${styles.entry} ${entry.kind === 'group' ? styles.entryGroup : ''}`}
      aria-current={entry.key === current.key}
      aria-expanded={expanded}
      onClick={() => onSelect(entry.key)}
    >
      <span className={entry.kind === 'group' ? styles.groupName : styles.entryName}>
        {entry.label}
      </span>
      <span className={styles.entryMeta}>
        <span className={styles.chip}>{entry.chip}</span>
        {entry.root}
      </span>
    </button>
  )
}

export function ReviewPage() {
  const queue = useApi('review', api.review)
  const ignored = useApi('review-ignored', api.ignoredFolders)
  useLiveEvents(['match.needs_review', 'scan.finished'], queue.reload, 1000)
  const [selected, setSelected] = useState<string | null>(null)
  const [collapsed, setCollapsed] = useState<string | null>(null)

  if (queue.error) {
    return <Failed error={queue.error} onRetry={queue.reload} />
  }
  if (!queue.data) {
    return <Loading />
  }
  const list = entries(queue.data)
  const current = list.find((e) => e.key === selected) ?? list[0]
  // A series' files show while it or one of them is selected, and it stays
  // selected after deciding one of them.
  const group = current && ('group' in current && current.group ? current.group : current.key)
  const done = () => {
    setSelected(group?.startsWith('group:') ? group : null)
    queue.reload()
    ignored.reload()
  }
  // Clicking the selected series again folds its files away, and back.
  const select = (key: string) => {
    if (key === current?.key && current.kind === 'group') {
      setCollapsed(collapsed === key ? null : key)
    } else {
      setCollapsed(null)
    }
    setSelected(key)
  }

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
              {list
                .filter((e) => !('group' in e && e.group))
                .map((e) => (
                  <li key={e.key}>
                    <EntryButton
                      entry={e}
                      current={current}
                      expanded={
                        e.kind === 'group' ? group === e.key && collapsed !== e.key : undefined
                      }
                      onSelect={select}
                    />
                    {e.kind === 'group' && group === e.key && collapsed !== e.key && (
                      <ul aria-label={`${e.label} files`} className={styles.children}>
                        {list
                          .filter((c) => 'group' in c && c.group === e.key)
                          .map((c) => (
                            <li key={c.key}>
                              <EntryButton entry={c} current={current} onSelect={select} />
                            </li>
                          ))}
                      </ul>
                    )}
                  </li>
                ))}
            </ul>
            {/* Keyed so each entry starts with its own selection and picker state. */}
            {current.kind === 'group' ? (
              <GroupDetail
                key={current.key}
                title={current.label}
                files={current.files}
                onSelect={(id) => select(`file:${id}`)}
                onDone={done}
              />
            ) : (
              <Detail key={current.key} entry={current} onDone={done} />
            )}
          </div>
        )}
        {ignored.data && ignored.data.length > 0 && (
          <Ignored
            folders={ignored.data}
            onDone={() => {
              queue.reload()
              ignored.reload()
            }}
          />
        )}
      </div>
    </div>
  )
}
