import type { Root } from '../../api/client'
import { Link } from '../../components/Link'
import { rootStatus } from './roots'
import styles from './Libraries.module.css'

/** One folder in a list: its path and how it's doing, linking to its settings. */
export function RootRow({ root, libraryId }: { root: Root; libraryId: number }) {
  const status = rootStatus(root)
  return (
    <li>
      <Link
        to={`/manage/libraries/${libraryId}/roots/${root.id}`}
        className={`${styles.rootRow} ${root.online ? '' : styles.offline}`}
      >
        <span className={styles.identity}>
          <span className={styles.pathLine}>
            <span className={styles.rowPath}>{root.path}</span>
            {root.read_only && <span className={styles.policyTag}>Read-only</span>}
            {root.is_import_target && <span className={styles.tag}>New items</span>}
          </span>
          <span className={`${styles.status} ${styles[status.tone]}`}>
            <span className={styles.dot} aria-hidden="true" />
            {status.text}
          </span>
        </span>
        <span className={styles.chevron} aria-hidden="true">
          ›
        </span>
      </Link>
    </li>
  )
}
