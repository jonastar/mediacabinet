import { useEffect } from 'react'
import { api } from '../../api/client'
import { useApi } from '../../api/useApi'
import { Link } from '../../components/Link'
import { match } from '../../lib/router'
import { LibrariesPage } from '../LibrariesPage'
import { LibraryDetail } from '../libraries/LibraryDetail'
import { RootDetail } from '../libraries/RootDetail'
import { NotFoundPage } from '../NotFoundPage'
import { ReviewPage } from '../ReviewPage'
import { SettingsPage } from '../SettingsPage'
import { WantedPage } from '../WantedPage'
import { Overview } from './Overview'
import { USERS_CHANGED_EVENT, UsersPage } from './UsersPage'
import styles from './Manage.module.css'
import usersStyles from './Users.module.css'

const TABS = [
  { to: '/manage', label: 'Overview' },
  { to: '/manage/wanted', label: 'Wanted' },
  { to: '/manage/review', label: 'Review queue' },
  { to: '/manage/libraries', label: 'Libraries & roots' },
  { to: '/manage/users', label: 'Users' },
  { to: '/manage/settings', label: 'Settings' },
]

function isCurrent(to: string, path: string) {
  return to === '/manage' ? path === to : path === to || path.startsWith(`${to}/`)
}

function Section({ path }: { path: string }) {
  if (path === '/manage') {
    return <Overview />
  }
  if (path === '/manage/wanted') {
    return <WantedPage />
  }
  if (path === '/manage/review') {
    return <ReviewPage />
  }
  if (path === '/manage/users') {
    return <UsersPage />
  }
  if (path === '/manage/settings') {
    return <SettingsPage />
  }
  if (path === '/manage/libraries') {
    return <LibrariesPage />
  }
  const library = match('/manage/libraries/:id', path)
  if (library && /^\d+$/.test(library.id)) {
    return <LibraryDetail key={library.id} id={Number(library.id)} />
  }
  const root = match('/manage/libraries/:library/roots/:id', path)
  if (root && /^\d+$/.test(root.library) && /^\d+$/.test(root.id)) {
    return <RootDetail key={root.id} libraryId={Number(root.library)} id={Number(root.id)} />
  }
  return <NotFoundPage />
}

/** How many people are waiting to be let in. */
function useWaiting() {
  const users = useApi('users', api.users)
  const { reload } = users
  useEffect(() => {
    window.addEventListener(USERS_CHANGED_EVENT, reload)
    return () => window.removeEventListener(USERS_CHANGED_EVENT, reload)
  }, [reload])
  return users.data?.filter((u) => u.status === 'pending').length ?? 0
}

/** Everything admins look after, one tab per section; under `/manage`. */
export function ManagePage({ path }: { path: string }) {
  const current = path.replace(/\/+$/, '') || '/manage'
  const waiting = useWaiting()
  return (
    <>
      <nav aria-label="Manage" className={styles.tabs}>
        {TABS.map((t) => (
          <Link
            key={t.to}
            to={t.to}
            className={styles.tab}
            aria-current={isCurrent(t.to, current) ? 'page' : undefined}
          >
            {t.label}
            {t.to === '/manage/users' && waiting > 0 && (
              <span className={usersStyles.tabCount} title={`${waiting} waiting to be let in`}>
                {waiting}
              </span>
            )}
          </Link>
        ))}
      </nav>
      <Section path={current} />
    </>
  )
}
