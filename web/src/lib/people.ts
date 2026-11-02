import type { About, Credit } from '../api/client'

/** Whether `AboutSections` has anything to show. */
export function hasAbout(about: About | null | undefined, cast?: Credit[]) {
  if (!about) {
    return false
  }
  return (
    (cast ?? about.cast).length > 0 ||
    about.guests.length > 0 ||
    about.crew.length > 0 ||
    about.networks.length > 0 ||
    about.studios.length > 0
  )
}
