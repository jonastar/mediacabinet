import type { About, Credit } from '../api/client'

/** The names credited with any of these jobs, each once: `James Cameron, Jon Landau`. */
export function crewNames(about: About | null | undefined, jobs: string[]): string | null {
  const people = (about?.crew ?? []).filter((c: Credit) => c.role && jobs.includes(c.role))
  const names = [...new Set(people.map((c) => c.name))]
  return names.length > 0 ? names.join(', ') : null
}
