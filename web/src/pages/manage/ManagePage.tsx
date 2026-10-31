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
import styles from './Manage.module.css'

const TABS = [
  { to: '/manage', label: 'Overview' },
  { to: '/manage/wanted', label: 'Wanted' },
  { to: '/manage/review', label: 'Review queue' },
  { to: '/manage/libraries', label: 'Libraries & roots' },
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

/** Everything admins look after, one tab per section; under `/manage`. */
export function ManagePage({ path }: { path: string }) {
  const current = path.replace(/\/+$/, '') || '/manage'
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
          </Link>
        ))}
      </nav>
      <Section path={current} />
    </>
  )
}
