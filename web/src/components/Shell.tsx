import type { ReactNode } from 'react'
import { api, type Library } from '../api/client'
import { useApi } from '../api/useApi'
import { useLiveEvents } from '../lib/live'
import { usePath } from '../lib/router'
import { Brand } from './Brand'
import { Link } from './Link'
import { useMode } from '../lib/prefs'
import { signOut, useUser } from '../lib/session'
import { Segmented } from './Segmented'
import {
  ActivityIcon,
  CalendarIcon,
  FolderIcon,
  HomeIcon,
  MoviesIcon,
  ReviewIcon,
  SettingsIcon,
  TvIcon,
  WantedIcon,
} from './Icons'
import styles from './Shell.module.css'

interface NavItem {
  to: string
  label: string
  icon: ReactNode
}

const WATCH_NAV: NavItem[] = [
  { to: '/', label: 'Home', icon: <HomeIcon /> },
  { to: '/calendar', label: 'Calendar', icon: <CalendarIcon /> },
  { to: '/activity', label: 'Activity', icon: <ActivityIcon /> },
]

const MANAGE_NAV: NavItem[] = [
  { to: '/wanted', label: 'Wanted', icon: <WantedIcon /> },
  { to: '/review', label: 'Review queue', icon: <ReviewIcon /> },
  { to: '/libraries', label: 'Libraries & roots', icon: <FolderIcon /> },
]

const SETTINGS: NavItem = { to: '/settings', label: 'Settings', icon: <SettingsIcon /> }

function libraryItem(library: Library): NavItem {
  return {
    to: `/library/${library.id}`,
    label: library.name,
    icon: library.kind === 'movies' ? <MoviesIcon /> : <TvIcon />,
  }
}

function isCurrent(to: string, path: string) {
  return to === '/' ? path === '/' : path === to || path.startsWith(`${to}/`)
}

function NavLink({ item, path }: { item: NavItem; path: string }) {
  return (
    <Link
      to={item.to}
      className={styles.link}
      aria-current={isCurrent(item.to, path) ? 'page' : undefined}
    >
      {item.icon}
      {item.label}
    </Link>
  )
}

export function Shell({ children }: { children: ReactNode }) {
  const path = usePath()
  const [mode, setMode] = useMode()
  const user = useUser()
  const libraries = useApi('libraries', api.libraries)
  useLiveEvents(['library.added', 'library.removed'], libraries.reload)

  return (
    <div className={styles.shell}>
      <nav aria-label="Main" className={styles.sidebar}>
        <Link to="/" className={styles.brand}>
          <Brand />
        </Link>
        {WATCH_NAV.map((item) => (
          <NavLink key={item.to} item={item} path={path} />
        ))}
        {libraries.data && libraries.data.length > 0 && (
          <>
            <div className={styles.divider} />
            <span className={styles.section}>Libraries</span>
            {libraries.data.map((library) => (
              <NavLink key={library.id} item={libraryItem(library)} path={path} />
            ))}
          </>
        )}
        {mode === 'manage' && (
          <>
            <div className={styles.divider} />
            <span className={styles.section}>Manage</span>
            {MANAGE_NAV.map((item) => (
              <NavLink key={item.to} item={item} path={path} />
            ))}
          </>
        )}
        <div className={styles.bottom}>
          {mode === 'manage' && <NavLink item={SETTINGS} path={path} />}
        </div>
        <div className={styles.user}>
          <span className={styles.who}>
            <span className={styles.avatar} aria-hidden="true">
              {user.name.slice(0, 1).toUpperCase()}
            </span>
            <span className={styles.name}>
              {user.name}
              {user.is_admin && <span className={styles.role}> · admin</span>}
            </span>
          </span>
          <span className={styles.accountLinks}>
            <Link to="/account" className={styles.accountLink}>
              Account
            </Link>
            <button type="button" className={styles.accountLink} onClick={() => signOut()}>
              Sign out
            </button>
          </span>
          {user.is_admin && (
            <Segmented
              label="Mode"
              variant="track"
              value={mode}
              onChange={setMode}
              options={[
                { value: 'watch', label: 'Watch' },
                { value: 'manage', label: 'Manage' },
              ]}
            />
          )}
        </div>
      </nav>
      <main className={styles.main}>{children}</main>
    </div>
  )
}
