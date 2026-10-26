import { Shell } from './components/Shell'
import { CalendarPage } from './pages/CalendarPage'
import { HomePage } from './pages/HomePage'
import { LibrariesPage } from './pages/LibrariesPage'
import { MoviePage } from './pages/MoviePage'
import { MoviesPage } from './pages/MoviesPage'
import { SearchPage } from './pages/SearchPage'
import { SettingsPage } from './pages/SettingsPage'
import { ReviewPage } from './pages/ReviewPage'
import { NotFoundPage } from './pages/NotFoundPage'
import { SeriesPage } from './pages/SeriesPage'
import { ShowsPage } from './pages/ShowsPage'
import { WantedPage } from './pages/WantedPage'
import { match, usePath } from './lib/router'
import { checkSession, useSession } from './lib/session'
import { AuthPage } from './pages/AuthPage'
import { AccountPage } from './pages/AccountPage'
import { PlayerPage } from './pages/PlayerPage'
import { Failed, Loading } from './components/Status'

/** Pages only admins can use. */
const MANAGE_PATHS = ['/wanted', '/review', '/libraries', '/settings']

function Page({ path, admin }: { path: string; admin: boolean }) {
  if (!admin && MANAGE_PATHS.includes(path)) {
    return <NotFoundPage />
  }
  if (path === '/account') {
    return <AccountPage />
  }
  if (path === '/') {
    return <HomePage />
  }
  if (path === '/movies') {
    return <MoviesPage />
  }
  const movie = match('/movies/:id', path)
  if (movie && /^\d+$/.test(movie.id)) {
    return <MoviePage key={movie.id} id={Number(movie.id)} />
  }
  if (path === '/wanted') {
    return <WantedPage />
  }
  if (path === '/calendar') {
    return <CalendarPage />
  }
  if (path === '/settings') {
    return <SettingsPage />
  }
  if (path === '/search') {
    return <SearchPage />
  }
  if (path === '/review') {
    return <ReviewPage />
  }
  if (path === '/libraries') {
    return <LibrariesPage />
  }
  if (path === '/tv') {
    return <ShowsPage />
  }
  const series = match('/tv/:id', path)
  if (series && /^\d+$/.test(series.id)) {
    // Keyed so switching series starts from a clean state.
    return <SeriesPage key={series.id} id={Number(series.id)} />
  }
  return <NotFoundPage />
}

checkSession()

export default function App() {
  const path = usePath()
  const session = useSession()
  if (session.state === 'checking') {
    return <Loading />
  }
  if (session.state === 'unreachable') {
    return <Failed error={session.error} onRetry={checkSession} />
  }
  if (session.state !== 'signedIn') {
    return <AuthPage setup={session.state === 'setup'} />
  }
  const play = match('/play/:id', path)
  if (play && /^\d+$/.test(play.id)) {
    // Keyed so the next episode starts a fresh player.
    return <PlayerPage key={play.id} id={Number(play.id)} />
  }
  return (
    <Shell>
      <Page path={path} admin={session.user.is_admin} />
    </Shell>
  )
}
