import { api } from '../api/client'
import { useApi } from '../api/useApi'
import { Failed, Loading } from '../components/Status'
import { MoviePage } from './MoviePage'
import { MoviesPage } from './MoviesPage'
import { NotFoundPage } from './NotFoundPage'
import { SeriesPage } from './SeriesPage'
import { ShowsPage } from './ShowsPage'

/** One library's movies or shows, as linked from the sidebar. */
export function LibraryPage({ id }: { id: number }) {
  const libraries = useApi('libraries', api.libraries)
  if (libraries.error) {
    return <Failed error={libraries.error} onRetry={libraries.reload} />
  }
  if (!libraries.data) {
    return <Loading />
  }
  const library = libraries.data.find((l) => l.id === id)
  if (!library) {
    return <NotFoundPage />
  }
  if (library.kind === 'movies') {
    return <MoviesPage library={library} />
  }
  return <ShowsPage library={library} />
}

/** A series or movie under its library: `/library/:library/:id`. */
export function LibraryItemPage({ libraryId, id }: { libraryId: number; id: number }) {
  const libraries = useApi('libraries', api.libraries)
  if (libraries.error) {
    return <Failed error={libraries.error} onRetry={libraries.reload} />
  }
  if (!libraries.data) {
    return <Loading />
  }
  const library = libraries.data.find((l) => l.id === libraryId)
  if (!library) {
    return <NotFoundPage />
  }
  if (library.kind === 'movies') {
    return <MoviePage key={id} id={id} />
  }
  return <SeriesPage key={id} id={id} />
}
