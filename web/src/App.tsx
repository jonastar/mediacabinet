import { Shell } from './components/Shell'
import { HomePage } from './pages/HomePage'
import { LibrariesPage } from './pages/LibrariesPage'
import { MoviePage } from './pages/MoviePage'
import { MoviesPage } from './pages/MoviesPage'
import { SearchPage } from './pages/SearchPage'
import { ReviewPage } from './pages/ReviewPage'
import { NotFoundPage, PlaceholderPage } from './pages/PlaceholderPage'
import { SeriesPage } from './pages/SeriesPage'
import { ShowsPage } from './pages/ShowsPage'
import { match, usePath } from './lib/router'

function Page({ path }: { path: string }) {
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
  const placeholders: Record<string, [string, string]> = {
    '/calendar': ['Calendar', 'Phase 5'],
    '/wanted': ['Wanted', 'Phase 5'],
  }
  const placeholder = placeholders[path.replace(/\/+$/, '')]
  if (placeholder) {
    return <PlaceholderPage title={placeholder[0]} phase={placeholder[1]} />
  }
  return <NotFoundPage />
}

export default function App() {
  const path = usePath()
  return (
    <Shell>
      <Page path={path} />
    </Shell>
  )
}
