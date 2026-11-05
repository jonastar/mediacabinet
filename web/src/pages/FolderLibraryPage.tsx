import { api, artworkUrl, type FolderVideo, type Library } from '../api/client'
import { useApi } from '../api/useApi'
import { CoverImage } from '../components/Artwork'
import { FolderIcon, PlayIcon } from '../components/Icons'
import { Link } from '../components/Link'
import { Failed, Loading } from '../components/Status'
import { runtime } from '../lib/format'
import { useLiveEvents } from '../lib/live'
import { useMode } from '../lib/prefs'
import { folderPath, itemPath, useQueryParam } from '../lib/router'
import pageStyles from './Page.module.css'
import styles from './FolderLibraryPage.module.css'

/** Sorts `Clip 2` before `Clip 10`. */
const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

function count(n: number, noun: string) {
  return `${n.toLocaleString()} ${noun}${n === 1 ? '' : 's'}`
}

function videoState(v: FolderVideo) {
  if (v.availability !== 'available') {
    return 'File missing'
  }
  if (v.watch?.played) {
    return 'Watched'
  }
  return (v.watch?.position_seconds ?? 0) > 0 ? 'In progress' : null
}

/** The library and each folder down to this one, as links. */
function Breadcrumbs({ library, path }: { library: Library; path: string }) {
  const levels = path.split('/')
  return (
    <nav aria-label="Folder" className={styles.crumbs}>
      <Link to={folderPath(library.id, '')}>{library.name}</Link>
      {levels.map((name, i) => {
        const to = levels.slice(0, i + 1).join('/')
        return (
          <span key={to} className={styles.crumb}>
            <span aria-hidden="true">/</span>
            {i === levels.length - 1 ? (
              <span aria-current="page">{name}</span>
            ) : (
              <Link to={folderPath(library.id, to)}>{name}</Link>
            )}
          </span>
        )
      })}
    </nav>
  )
}

/** A folder library: one folder's subfolders and videos, browsed as they are on disk. */
export function FolderLibraryPage({ library }: { library: Library }) {
  const [mode] = useMode()
  const path = useQueryParam('path') ?? ''
  const page = useApi(`folder:${library.id}:${path}`, () => api.browseLibrary(library.id, path))
  useLiveEvents(['scan.finished'], page.reload, 1000)
  if (page.error) {
    return <Failed error={page.error} onRetry={page.reload} />
  }
  if (!page.data) {
    return <Loading />
  }
  // Files that have gone missing are only of interest to whoever manages the library.
  const manage = mode === 'manage'
  const folders = page.data.folders
    .filter((f) => manage || f.videos > 0)
    .sort((a, b) => byName.compare(a.name, b.name))
  const videos = page.data.videos
    .filter((v) => manage || v.availability === 'available')
    .sort((a, b) => byName.compare(a.title, b.title))

  return (
    <div className={pageStyles.page}>
      {path ? (
        <Breadcrumbs library={library} path={path} />
      ) : (
        <h1 className={pageStyles.title}>{library.name}</h1>
      )}
      {folders.length === 0 && videos.length === 0 && (
        <p className={pageStyles.muted}>
          {path ? 'No videos in this folder.' : 'No videos in this library yet.'}
        </p>
      )}
      <ul className={styles.list}>
        {folders.map((f) => (
          <li key={f.path}>
            <Link to={folderPath(library.id, f.path)} className={styles.row}>
              <span className={styles.icon}>
                <FolderIcon />
              </span>
              <span className={styles.name}>{f.name}</span>
              <span className={styles.meta}>
                {[
                  count(f.videos, 'video'),
                  f.watched > 0 ? `${f.watched.toLocaleString()} watched` : null,
                  manage && f.missing > 0 ? `${f.missing.toLocaleString()} missing` : null,
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </Link>
          </li>
        ))}
        {videos.map((v) => {
          const available = v.availability === 'available'
          return (
            <li key={v.id} className={`${styles.row} ${styles.video}`}>
              {available ? (
                <Link
                  to={`/play/${v.id}`}
                  className={`${styles.thumb} ${styles.play}`}
                  aria-label={`Play ${v.title}`}
                >
                  <CoverImage src={artworkUrl(v, 'thumbnail')} />
                  <PlayIcon />
                </Link>
              ) : (
                <span className={styles.thumb}>
                  <CoverImage src={artworkUrl(v, 'thumbnail')} />
                </span>
              )}
              <Link to={itemPath('movie', v.id, v.library_id)} className={styles.name}>
                {v.title}
              </Link>
              <span className={styles.meta}>
                {[videoState(v), runtime(v.runtime_seconds)].filter(Boolean).join(' · ')}
              </span>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
