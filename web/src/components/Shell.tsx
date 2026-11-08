import type { ReactNode } from 'react'
import { api, type Library, type LibraryKind } from '../api/client'
import { useApi } from '../api/useApi'
import { useLiveEvents } from '../lib/live'
import { usePath } from '../lib/router'
import { Brand } from './Brand'
import { Link } from './Link'
import { signOut, useUser } from '../lib/session'
import {
  ActivityIcon,
  CalendarIcon,
  FolderIcon,
  HomeIcon,
  MoviesIcon,
  RemoteIcon,
  SettingsIcon,
  SignOutIcon,
  TvIcon,
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

const MANAGE: NavItem = { to: '/manage', label: 'Manage', icon: <SettingsIcon /> }

const LIBRARY_ICONS: Record<LibraryKind, ReactNode> = {
  tv: <TvIcon />,
  movies: <MoviesIcon />,
  folders: <FolderIcon />,
}

function libraryItem(library: Library): NavItem {
  return {
    to: `/library/${library.id}`,
    label: library.name,
    icon: LIBRARY_ICONS[library.kind as LibraryKind],
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
        {/* The TV app, served beside this one: a page load, not a route here. */}
        <a
          href="/tv-mode/"
          className={styles.link}
          title="Browse and watch with a remote or arrow keys"
        >
          <RemoteIcon />
          TV mode
        </a>
        {libraries.data && libraries.data.length > 0 && (
          <>
            <div className={styles.divider} />
            <span className={styles.section}>Libraries</span>
            {libraries.data.map((library) => (
              <NavLink key={library.id} item={libraryItem(library)} path={path} />
            ))}
          </>
        )}
        <div className={styles.bottom}>
          {user.is_admin && <NavLink item={MANAGE} path={path} />}
        </div>
        <div className={styles.user}>
          <Link
            to="/account"
            className={styles.who}
            title="Account"
            aria-current={isCurrent('/account', path) ? 'page' : undefined}
          >
            <span className={styles.avatar} aria-hidden="true">
              {user.name.slice(0, 1).toUpperCase()}
            </span>
            <span className={styles.identity}>
              <span className={styles.name}>{user.name}</span>
              <span className={styles.role}>{user.is_admin ? 'Admin' : 'Account'}</span>
            </span>
          </Link>
          <button
            type="button"
            className={styles.signOut}
            title="Sign out"
            aria-label="Sign out"
            onClick={() => signOut()}
          >
            <SignOutIcon />
          </button>
        </div>
      </nav>
      <main className={styles.main}>{children}</main>
    </div>
  )
}
