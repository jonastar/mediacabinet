import { api } from '../api/client'
import { useApi } from '../api/useApi'
import { BackIcon } from './Icons'
import { folderPath } from '../lib/router'
import { Link } from './Link'

/**
 * The back link on a series or movie page: to its library, or to all
 * `fallback` items when it has none. With `folder`, to that folder of a folder library.
 */
export function LibraryBack({
  libraryId,
  fallback,
  folder,
  className,
}: {
  libraryId: number | null | undefined
  fallback: { to: string; label: string }
  folder?: string
  className?: string
}) {
  const libraries = useApi('libraries', api.libraries)
  const library = libraryId ? libraries.data?.find((l) => l.id === libraryId) : undefined
  return (
    <Link to={library ? folderPath(library.id, folder ?? '') : fallback.to} className={className}>
      <BackIcon />
      {folder?.split('/').pop() || library?.name || fallback.label}
    </Link>
  )
}
