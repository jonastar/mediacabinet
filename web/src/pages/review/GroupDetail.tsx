import { useState } from 'react'
import { api, type FileReview } from '../../api/client'
import { barColor, pct } from './confidence'
import styles from './Review.module.css'

/** Above this, a top candidate is ticked to accept to begin with. */
const STRONG = 0.7

const lastPart = (path: string) => path.split('/').pop() ?? path

/**
 * Every file of one series waiting for review, to decide together: accept the
 * top candidate of each ticked file in one go. Files without a candidate are
 * listed but left for one by one.
 */
export function GroupDetail({
  title,
  files,
  onSelect,
  onDone,
}: {
  title: string
  files: FileReview[]
  onSelect: (fileId: number) => void
  onDone: () => void
}) {
  const [ticked, setTicked] = useState(
    () =>
      new Set(
        files.filter((f) => (f.candidates[0]?.confidence ?? 0) >= STRONG).map((f) => f.file_id),
      ),
  )
  const [progress, setProgress] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const withCandidate = files.filter((f) => f.candidates.length > 0)
  const chosen = withCandidate.filter((f) => ticked.has(f.file_id))
  const allTicked = withCandidate.length > 0 && chosen.length === withCandidate.length

  const toggle = (id: number) => {
    const next = new Set(ticked)
    if (next.has(id)) {
      next.delete(id)
    } else {
      next.add(id)
    }
    setTicked(next)
  }

  const acceptAll = async () => {
    setError(null)
    let done = 0
    try {
      for (const f of chosen) {
        setProgress(done)
        await api.acceptCandidate(f.candidates[0].id, true)
        done += 1
      }
    } catch (e) {
      setError(`${done} of ${chosen.length} accepted, then: ${(e as Error).message}`)
    } finally {
      setProgress(null)
      onDone()
    }
  }

  const left = files.length - withCandidate.length
  return (
    <section aria-labelledby="review-detail" className={styles.detail}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
        <h2 id="review-detail" className={styles.label}>
          Series
        </h2>
        <p className={styles.groupTitle}>{title}</p>
      </div>
      <div className={styles.why}>
        {files.length} files of {title} need a decision. Tick the ones whose top candidate is right
        and accept them together; each link is pinned and survives every rescan. Open a file for its
        details or another match.
        {left > 0 &&
          ` ${left} ${left === 1 ? 'has' : 'have'} no candidate and must be matched one by one.`}
      </div>
      <fieldset className={styles.candidates}>
        <legend className={styles.label}>
          <label className={styles.tickAll}>
            <input
              type="checkbox"
              checked={allTicked}
              disabled={withCandidate.length === 0 || progress !== null}
              onChange={() =>
                setTicked(allTicked ? new Set() : new Set(withCandidate.map((f) => f.file_id)))
              }
            />
            Top candidates
          </label>
        </legend>
        {files.map((f) => {
          const top = f.candidates[0]
          return (
            <div key={f.file_id} className={`${styles.candidate} ${styles.groupFile}`}>
              <input
                type="checkbox"
                aria-label={`Accept the top candidate for ${lastPart(f.relative_path)}`}
                checked={ticked.has(f.file_id) && top !== undefined}
                disabled={!top || progress !== null}
                onChange={() => toggle(f.file_id)}
              />
              <span className={styles.candidateText}>
                <button
                  type="button"
                  className={styles.fileLink}
                  onClick={() => onSelect(f.file_id)}
                >
                  {lastPart(f.relative_path)}
                </button>
                <span className={styles.candidateTitle}>
                  {top ? `→ ${top.title}` : 'No candidate'}
                </span>
                {top && <span className={styles.candidateReason}>{top.reason}</span>}
              </span>
              {top && (
                <span className={styles.confidence}>
                  <span className={styles.confidenceLine}>
                    <b>{pct(top.confidence)}</b>
                  </span>
                  <span className={styles.bar} aria-hidden="true">
                    <span
                      style={{ width: pct(top.confidence), background: barColor(top.confidence) }}
                    />
                  </span>
                </span>
              )}
            </div>
          )
        })}
      </fieldset>
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        <button
          type="button"
          className={`${styles.button} ${styles.primary}`}
          disabled={chosen.length === 0 || progress !== null}
          onClick={acceptAll}
        >
          {progress !== null
            ? `Accepting ${progress + 1} of ${chosen.length}…`
            : `Accept and pin ${chosen.length} ${chosen.length === 1 ? 'file' : 'files'}`}
        </button>
      </div>
    </section>
  )
}
