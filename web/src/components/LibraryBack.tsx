import { api } from '../api/client'
import { useApi } from '../api/useApi'
import { BackIcon } from './Icons'
import { Link } from './Link'

/**
 * The back link on a series or movie page: to its library, or to all
 * `fallback` items when it has none.
 */
export function LibraryBack({
  libraryId,
  fallback,
  className,
}: {
  libraryId: number | null | undefined
  fallback: { to: string; label: string }
  className?: string
}) {
  const libraries = useApi('libraries', api.libraries)
  const library = libraryId ? libraries.data?.find((l) => l.id === libraryId) : undefined
  return (
    <Link to={library ? `/library/${library.id}` : fallback.to} className={className}>
      <BackIcon />
      {library?.name ?? fallback.label}
    </Link>
  )
}
