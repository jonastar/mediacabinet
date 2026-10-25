import type { ReactNode } from 'react'
import { usePath } from '../lib/router'
import { Link } from './Link'
import { useMode } from '../lib/prefs'
import { Segmented } from './Segmented'
import {
  CalendarIcon,
  FolderIcon,
  HomeIcon,
  LogoIcon,
  MoviesIcon,
  ReviewIcon,
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
  { to: '/movies', label: 'Movies', icon: <MoviesIcon /> },
  { to: '/tv', label: 'TV Shows', icon: <TvIcon /> },
  { to: '/calendar', label: 'Calendar', icon: <CalendarIcon /> },
]

const MANAGE_NAV: NavItem[] = [
  { to: '/wanted', label: 'Wanted', icon: <WantedIcon /> },
  { to: '/review', label: 'Review queue', icon: <ReviewIcon /> },
  { to: '/libraries', label: 'Libraries & roots', icon: <FolderIcon /> },
]

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

  return (
    <div className={styles.shell}>
      <nav aria-label="Main" className={styles.sidebar}>
        <Link to="/" className={styles.brand}>
          <LogoIcon />
          Media Cabinet
        </Link>
        {WATCH_NAV.map((item) => (
          <NavLink key={item.to} item={item} path={path} />
        ))}
        {mode === 'manage' && (
          <>
            <div className={styles.divider} />
            <span className={styles.section}>Manage</span>
            {MANAGE_NAV.map((item) => (
              <NavLink key={item.to} item={item} path={path} />
            ))}
          </>
        )}
        <div className={styles.user}>
          <span className={styles.who}>
            <span className={styles.avatar} aria-hidden="true">
              D
            </span>
            <span>
              demo <span className={styles.role}>· admin</span>
            </span>
          </span>
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
        </div>
      </nav>
      <main className={styles.main}>{children}</main>
    </div>
  )
}
