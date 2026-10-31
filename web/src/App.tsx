import { Shell } from './components/Shell'
import { ActivityPage } from './pages/ActivityPage'
import { CalendarPage } from './pages/CalendarPage'
import { EpisodePage } from './pages/EpisodePage'
import { HomePage } from './pages/HomePage'
import { LibraryItemPage, LibraryPage } from './pages/LibraryPage'
import { MoviePage } from './pages/MoviePage'
import { ManagePage } from './pages/manage/ManagePage'
import { MoviesPage } from './pages/MoviesPage'
import { SearchPage } from './pages/SearchPage'
import { NotFoundPage } from './pages/NotFoundPage'
import { SeriesPage } from './pages/SeriesPage'
import { ShowsPage } from './pages/ShowsPage'
import { match, usePath } from './lib/router'
import { checkSession, useSession } from './lib/session'
import { AuthPage } from './pages/AuthPage'
import { AccountPage } from './pages/AccountPage'
import { PlayerPage } from './pages/PlayerPage'
import { Failed } from './components/Status'
import { BootScreen } from './components/Brand'

function Page({ path, admin }: { path: string; admin: boolean }) {
  if (path === '/manage' || path.startsWith('/manage/')) {
    // Only admins can manage; to anyone else these pages don't exist.
    return admin ? <ManagePage path={path} /> : <NotFoundPage />
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
  if (path === '/activity') {
    return <ActivityPage />
  }
  if (path === '/calendar') {
    return <CalendarPage />
  }
  if (path === '/search') {
    return <SearchPage />
  }
  const library = match('/library/:id', path)
  if (library && /^\d+$/.test(library.id)) {
    return <LibraryPage key={library.id} id={Number(library.id)} />
  }
  const libraryItem = match('/library/:library/:id', path)
  if (libraryItem && /^\d+$/.test(libraryItem.library) && /^\d+$/.test(libraryItem.id)) {
    return <LibraryItemPage libraryId={Number(libraryItem.library)} id={Number(libraryItem.id)} />
  }
  const libraryEpisode = match('/library/:library/:series/:id', path)
  if (libraryEpisode && /^\d+$/.test(libraryEpisode.series) && /^\d+$/.test(libraryEpisode.id)) {
    return (
      <EpisodePage
        key={libraryEpisode.id}
        seriesId={Number(libraryEpisode.series)}
        id={Number(libraryEpisode.id)}
      />
    )
  }
  if (path === '/tv') {
    return <ShowsPage />
  }
  const series = match('/tv/:id', path)
  if (series && /^\d+$/.test(series.id)) {
    // Keyed so switching series starts from a clean state.
    return <SeriesPage key={series.id} id={Number(series.id)} />
  }
  const episode = match('/tv/:series/:id', path)
  if (episode && /^\d+$/.test(episode.series) && /^\d+$/.test(episode.id)) {
    return (
      <EpisodePage key={episode.id} seriesId={Number(episode.series)} id={Number(episode.id)} />
    )
  }
  return <NotFoundPage />
}

checkSession()

export default function App() {
  const path = usePath()
  const session = useSession()
  if (session.state === 'checking') {
    return <BootScreen />
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
