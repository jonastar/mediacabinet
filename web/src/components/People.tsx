import { personPhotoUrl, type About, type Credit, type Rating } from '../api/client'
import { seriesTone } from '../lib/format'
import { CoverImage } from './Artwork'
import { Link } from './Link'
import styles from './People.module.css'

const SOURCES: Record<string, string> = {
  imdb: 'IMDb',
  rotten_tomatoes: 'Rotten Tomatoes',
  metacritic: 'Metacritic',
  tmdb: 'TMDB',
}

/** A score as its site shows it: 8.1, 94%, 74. */
function score(r: Rating) {
  if (r.scale === 100) {
    return r.source === 'metacritic' ? `${Math.round(r.value)}` : `${Math.round(r.value)}%`
  }
  return (r.value * (10 / r.scale)).toFixed(1)
}

/** The age rating and each site's score, for a hero. */
export function Ratings({ about }: { about: About | null | undefined }) {
  if (!about || (!about.content_rating && about.ratings.length === 0)) {
    return null
  }
  return (
    <ul className={styles.ratings} aria-label="Ratings">
      {about.content_rating && (
        <li className={styles.certificate} title="Age rating">
          {about.content_rating}
        </li>
      )}
      {about.ratings.map((r) => (
        <li
          key={r.source}
          className={styles.rating}
          title={r.votes ? `${r.votes.toLocaleString()} votes` : undefined}
        >
          <b>{score(r)}</b> {SOURCES[r.source] ?? r.source}
        </li>
      ))}
    </ul>
  )
}

function initials(name: string) {
  const words = name.split(/\s+/).filter(Boolean)
  return (words[0]?.[0] ?? '') + (words.length > 1 ? words[words.length - 1][0] : '')
}

/** A photo, or initials over a colour when there's none. */
export function PersonPhoto({
  id,
  name,
  hasPhoto,
}: {
  id: number
  name: string
  hasPhoto: boolean
}) {
  return (
    <div className={styles.photo} style={{ background: seriesTone(name) }}>
      <span className={styles.initials} aria-hidden="true">
        {initials(name)}
      </span>
      <CoverImage src={personPhotoUrl({ id, has_photo: hasPhoto })} />
    </div>
  )
}

function PersonCard({ credit: c }: { credit: Credit }) {
  return (
    <Link to={`/people/${c.person_id}`} className={styles.card}>
      <PersonPhoto id={c.person_id} name={c.name} hasPhoto={c.has_photo} />
      <b className={styles.name}>{c.name}</b>
      {c.role && <span className={styles.role}>{c.role}</span>}
      {c.episodes != null && c.episodes > 0 && (
        <span className={styles.role}>
          {c.episodes} {c.episodes === 1 ? 'episode' : 'episodes'}
        </span>
      )}
    </Link>
  )
}

export function CastRail({ title, credits }: { title: string; credits: Credit[] }) {
  if (credits.length === 0) {
    return null
  }
  const id = `cast-${title.toLowerCase().replace(/\W+/g, '-')}`
  return (
    <section aria-labelledby={id} className={styles.section}>
      <h2 id={id}>{title}</h2>
      <div className={styles.rail}>
        {credits.map((c) => (
          <PersonCard key={`${c.person_id}-${c.role}`} credit={c} />
        ))}
      </div>
    </section>
  )
}

/** Crew jobs shown together, most prominent first. */
const CREW_GROUPS: [string, string[]][] = [
  ['Created by', ['Creator']],
  ['Directed by', ['Director']],
  ['Written by', ['Screenplay', 'Writer', 'Teleplay']],
  ['Story', ['Story', 'Original Story', 'Novel', 'Characters', 'Comic Book']],
  ['Music', ['Original Music Composer', 'Music']],
  ['Cinematography', ['Director of Photography']],
  ['Editing', ['Editor']],
  ['Produced by', ['Producer']],
]

function People({ credits }: { credits: Credit[] }) {
  return (
    <>
      {credits.map((c, i) => (
        <span key={c.person_id}>
          {i > 0 && ', '}
          <Link to={`/people/${c.person_id}`}>{c.name}</Link>
        </span>
      ))}
    </>
  )
}

/** Key crew, networks and studios as a list of facts. */
export function Facts({ about }: { about: About }) {
  const groups = CREW_GROUPS.map(([label, jobs]) => {
    const seen = new Set<number>()
    const people = about.crew.filter(
      (c) => c.role && jobs.includes(c.role) && !seen.has(c.person_id) && seen.add(c.person_id),
    )
    return [label, people] as const
  }).filter(([, people]) => people.length > 0)
  if (groups.length === 0 && about.networks.length === 0 && about.studios.length === 0) {
    return null
  }
  return (
    <dl className={styles.facts}>
      {groups.map(([label, people]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>
            <People credits={people} />
          </dd>
        </div>
      ))}
      {about.networks.length > 0 && (
        <div>
          <dt>{about.networks.length === 1 ? 'Network' : 'Networks'}</dt>
          <dd>{about.networks.join(', ')}</dd>
        </div>
      )}
      {about.studios.length > 0 && (
        <div>
          <dt>{about.studios.length === 1 ? 'Studio' : 'Studios'}</dt>
          <dd>{about.studios.join(', ')}</dd>
        </div>
      )}
    </dl>
  )
}

/** Cast, guest stars and the facts, for the bottom of a media page. */
export function AboutSections({ about, cast }: { about: About; cast?: Credit[] }) {
  const regulars = cast ?? about.cast
  return (
    <>
      <CastRail title="Cast" credits={regulars} />
      <CastRail title="Guest stars" credits={about.guests} />
      <Facts about={about} />
    </>
  )
}
