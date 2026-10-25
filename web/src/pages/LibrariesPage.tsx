import { useEffect, useState } from 'react'
import { api, type LibraryKind } from '../api/client'
import { useApi } from '../api/useApi'
import { Segmented } from '../components/Segmented'
import { Failed, Loading } from '../components/Status'
import { AddFolder } from './libraries/AddFolder'
import { NewLibrary } from './libraries/NewLibrary'
import { PlacementPanel } from './libraries/PlacementPanel'
import { RootCard } from './libraries/RootCard'
import { Legend } from './libraries/TemplateText'
import styles from './libraries/Libraries.module.css'

export function LibrariesPage() {
  const libraries = useApi('libraries', api.libraries)
  const [picked, setPicked] = useState<number | null>(null)
  const [adding, setAdding] = useState<'folder' | 'library' | null>(null)

  // While anything scans, refresh every second to show progress.
  const scanning = libraries.data?.some((l) => l.roots.some((r) => r.scanning)) ?? false
  const reload = libraries.reload
  useEffect(() => {
    if (!scanning) {
      return
    }
    const timer = window.setInterval(reload, 1000)
    return () => window.clearInterval(timer)
  }, [scanning, reload])

  if (libraries.error) {
    return <Failed error={libraries.error} onRetry={libraries.reload} />
  }
  if (!libraries.data) {
    return <Loading />
  }
  const all = libraries.data
  if (all.length === 0) {
    return (
      <div className={styles.page}>
        <div className={styles.inner}>
          <div className={styles.heading}>
            <h1>Libraries & roots</h1>
            <p className={styles.lead}>Start with a library, then add the folders that hold it.</p>
          </div>
          <NewLibrary
            onCreated={(l) => {
              setPicked(l.id)
              setAdding('folder')
              reload()
            }}
          />
        </div>
      </div>
    )
  }

  const library = all.find((l) => l.id === picked) ?? all[0]
  const kind = library.kind as LibraryKind
  // Changes to roots change the placement example; refetch it when they do.
  const version = library.roots
    .map((r) => `${r.id}:${r.write_policy}:${r.layout_template}`)
    .join('|')

  const deleteLibrary = async () => {
    if (window.confirm(`Delete the library ${library.name}?`)) {
      await api.deleteLibrary(library.id)
      setPicked(null)
      reload()
    }
  }

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        {all.length > 1 && (
          <Segmented
            label="Library"
            variant="pills"
            value={String(library.id)}
            onChange={(id) => {
              setPicked(Number(id))
              setAdding(null)
            }}
            options={all.map((l) => ({ value: String(l.id), label: l.name }))}
          />
        )}
        <div className={styles.header}>
          <div className={styles.heading}>
            <h1>{library.name} · roots</h1>
            <p className={styles.lead}>
              Each folder has its own layout and write policy. All of them feed one library.
            </p>
          </div>
          <div className={styles.headerActions}>
            <button type="button" className={styles.button} onClick={() => setAdding('library')}>
              New library
            </button>
            <button type="button" className={styles.button} onClick={() => setAdding('folder')}>
              Add a folder
            </button>
          </div>
        </div>

        {adding === 'library' && (
          <NewLibrary
            onCreated={(l) => {
              setPicked(l.id)
              setAdding('folder')
              reload()
            }}
            onCancel={() => setAdding(null)}
          />
        )}
        {adding === 'folder' && (
          <AddFolder
            library={library}
            onAdded={() => {
              setAdding(null)
              reload()
            }}
            onCancel={() => setAdding(null)}
          />
        )}

        {library.roots.length === 0 ? (
          adding !== 'folder' && (
            <div className={styles.panel}>
              <p className={styles.muted}>No folders in this library yet.</p>
              <div className={styles.actions}>
                <button
                  type="button"
                  className={`${styles.button} ${styles.primary}`}
                  onClick={() => setAdding('folder')}
                >
                  Add a folder
                </button>
                <button
                  type="button"
                  className={`${styles.button} ${styles.danger}`}
                  onClick={deleteLibrary}
                >
                  Delete library
                </button>
              </div>
            </div>
          )
        ) : (
          <ul className={styles.list}>
            {library.roots.map((r) => (
              <RootCard key={r.id} root={r} kind={kind} onChanged={reload} />
            ))}
          </ul>
        )}

        <Legend kind={library.kind} />
        <PlacementPanel libraryId={library.id} version={version} />
      </div>
    </div>
  )
}
