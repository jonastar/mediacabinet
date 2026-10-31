import type { Root, WritePolicy } from '../../api/client'
import { Link } from '../../components/Link'
import { POLICIES, rootStatus } from './roots'
import styles from './Libraries.module.css'

const POLICY_LABELS = Object.fromEntries(POLICIES) as Record<WritePolicy, string>

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
            <span className={styles.policyTag}>
              {POLICY_LABELS[root.write_policy as WritePolicy]}
            </span>
            {root.is_import_target && <span className={styles.tag}>Import target</span>}
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
