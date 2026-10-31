import { useMode } from '../lib/prefs'
import { useUser } from '../lib/session'
import styles from './DetailsToggle.module.css'

/** Shows or hides the admin detail (Manage mode) on media pages; admins only. */
export function DetailsToggle() {
  const user = useUser()
  const [mode, setMode] = useMode()
  if (!user.is_admin) {
    return null
  }
  const on = mode === 'manage'
  return (
    <button
      type="button"
      className={styles.toggle}
      aria-pressed={on}
      title="System states, files and settings that apply for everyone"
      onClick={() => setMode(on ? 'watch' : 'manage')}
    >
      <span className={styles.switch} aria-hidden="true" />
      Details
    </button>
  )
}
