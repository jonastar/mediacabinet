import { useState } from 'react'
import type { Library } from '../api/client'
import { Link } from '../components/Link'
import { Failed, Loading } from '../components/Status'
import { navigate } from '../lib/router'
import { NewLibrary } from './libraries/NewLibrary'
import { librarySummary } from './libraries/roots'
import { RootRow } from './libraries/RootRow'
import { useLibraries } from './libraries/useLibraries'
import styles from './libraries/Libraries.module.css'

/** Every library and its folders; each leads to its own settings. */
export function LibrariesPage() {
  const libraries = useLibraries()
  const [adding, setAdding] = useState(false)

  if (libraries.error) {
    return <Failed error={libraries.error} onRetry={libraries.reload} />
  }
  if (!libraries.data) {
    return <Loading />
  }
  const all = libraries.data
  const created = (l: Library) => navigate(`/manage/libraries/${l.id}?add=1`)

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <div className={styles.header}>
          <div className={styles.heading}>
            <h1>Libraries & roots</h1>
            <p className={styles.lead}>
              A library holds one kind of media, from one or more folders. Choose one to change it.
            </p>
          </div>
          {all.length > 0 && !adding && (
            <div className={styles.headerActions}>
              <button type="button" className={styles.button} onClick={() => setAdding(true)}>
                New library
              </button>
            </div>
          )}
        </div>

        {(adding || all.length === 0) && (
          <NewLibrary
            onCreated={created}
            onCancel={all.length > 0 ? () => setAdding(false) : undefined}
          />
        )}

        <ul className={styles.list}>
          {all.map((library) => {
            const noTarget =
              library.kind !== 'folders' &&
              library.roots.length > 0 &&
              !library.roots.some((r) => r.is_import_target)
            return (
              <li key={library.id} className={styles.libraryCard}>
                <Link to={`/manage/libraries/${library.id}`} className={styles.libraryHead}>
                  <span className={styles.identity}>
                    <span className={styles.libraryName}>{library.name}</span>
                    <span className={styles.muted}>{librarySummary(library)}</span>
                  </span>
                  {noTarget && <span className={styles.warnTag}>No import target</span>}
                  <span className={styles.chevron} aria-hidden="true">
                    ›
                  </span>
                </Link>
                {library.roots.length > 0 ? (
                  <ul className={styles.rootList} aria-label={`Folders of ${library.name}`}>
                    {library.roots.map((r) => (
                      <RootRow key={r.id} root={r} libraryId={library.id} />
                    ))}
                  </ul>
                ) : (
                  <p className={`${styles.muted} ${styles.empty}`}>
                    No folders yet.{' '}
                    <Link to={`/manage/libraries/${library.id}?add=1`} className={styles.link}>
                      Add a folder
                    </Link>
                  </p>
                )}
              </li>
            )
          })}
        </ul>
      </div>
    </div>
  )
}
