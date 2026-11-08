import { useMode } from '../lib/prefs'
import { useUser } from '../lib/session'
import styles from './EditModeToggle.module.css'

/** Switches media pages between viewing and editing (Manage mode); admins only. */
export function EditModeToggle() {
  const user = useUser()
  const [mode, setMode] = useMode()
  if (!user.is_admin) {
    return null
  }
  const on = mode === 'manage'
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      className={styles.toggle}
      title="System states, files and settings that apply for everyone"
      onClick={() => setMode(on ? 'watch' : 'manage')}
    >
      <span className={styles.switch} aria-hidden="true" />
      Edit mode
    </button>
  )
}
