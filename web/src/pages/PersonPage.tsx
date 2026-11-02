import { api, artworkUrl, type PersonCredit, type PersonDetail } from '../api/client'
import { useApi } from '../api/useApi'
import { CoverImage } from '../components/Artwork'
import { Link } from '../components/Link'
import { PersonPhoto } from '../components/People'
import { Failed, Loading } from '../components/Status'
import { episodeLabel, seriesTone } from '../lib/format'
import { episodePath, itemPath } from '../lib/router'
import styles from './PersonPage.module.css'

const dayFormat = new Intl.DateTimeFormat(undefined, {
  day: 'numeric',
  month: 'long',
  year: 'numeric',
  timeZone: 'UTC',
})

function day(date: string) {
  const d = new Date(`${date}T00:00:00Z`)
  return Number.isNaN(d.getTime()) ? date : dayFormat.format(d)
}

/** Whole years from `from` to `to` (both `YYYY-MM-DD`), or to today. */
function age(from: string, to?: string | null) {
  const [y1, m1, d1] = from.split('-').map(Number)
  const end = to ? to.split('-').map(Number) : null
  const now = new Date()
  const [y2, m2, d2] = end ?? [now.getFullYear(), now.getMonth() + 1, now.getDate()]
  const years = y2 - y1 - (m2 < m1 || (m2 === m1 && d2 < d1) ? 1 : 0)
  return Number.isFinite(years) ? years : null
}

/** "Born 4 March 1980 (46) in Osaka, Japan", or the dates of someone who died. */
function lifeLine(p: PersonDetail) {
  if (!p.birthday) {
    return p.birthplace ? `From ${p.birthplace}` : null
  }
  const years = age(p.birthday, p.deathday)
  const born = `Born ${day(p.birthday)}${p.birthplace ? ` in ${p.birthplace}` : ''}`
  if (p.deathday) {
    return `${born}. Died ${day(p.deathday)}${years != null ? ` (aged ${years})` : ''}`
  }
  return years != null ? `${born} · ${years} years old` : born
}

function CreditRow({ credit: c }: { credit: PersonCredit }) {
  const kind = c.kind === 'movie' ? 'movie' : 'series'
  const to = itemPath(kind, c.item_id, c.library_id)
  const count = c.episode_count ?? (c.episodes.length > 0 ? c.episodes.length : null)
  const line = [
    c.roles.join(', '),
    count ? `${count} ${count === 1 ? 'episode' : 'episodes'}` : null,
  ]
    .filter(Boolean)
    .join(' · ')
  return (
    <li className={styles.credit}>
      <Link to={to} className={styles.poster} style={{ background: seriesTone(c.title) }}>
        <CoverImage src={artworkUrl({ id: c.item_id, artwork: c.artwork }, 'poster')} alt="" />
      </Link>
      <div className={styles.creditBody}>
        <Link to={to} className={styles.creditTitle}>
          {c.title}
          {c.year && <span className={styles.year}> {c.year}</span>}
        </Link>
        {line && <span className={styles.roles}>{line}</span>}
        {c.episodes.length > 0 && (
          <ul className={styles.episodes}>
            {c.episodes.map((e) => (
              <li key={e.item_id}>
                <Link to={episodePath(c.item_id, e.item_id, c.library_id)}>
                  {e.number && <span className={styles.number}>{episodeLabel(e.number)}</span>}
                  {e.title}
                </Link>
                {e.role && <span className={styles.episodeRole}> · {e.role}</span>}
              </li>
            ))}
          </ul>
        )}
      </div>
    </li>
  )
}

/** Someone in the library: who they are, and everything here they're in. */
export function PersonPage({ id }: { id: number }) {
  const person = useApi(`person:${id}`, () => api.person(id))
  if (person.error) {
    return <Failed error={person.error} onRetry={person.reload} />
  }
  if (!person.data) {
    return <Loading />
  }
  const p = person.data
  const life = lifeLine(p)
  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <div className={styles.photo}>
          <PersonPhoto id={p.id} name={p.name} hasPhoto={p.has_photo} />
        </div>
        <div className={styles.intro}>
          {p.known_for && <span className={styles.eyebrow}>{p.known_for}</span>}
          <h1 className={styles.name}>{p.name}</h1>
          {life && <p className={styles.life}>{life}</p>}
          {p.biography &&
            p.biography.split(/\n\s*\n/).map((para, i) => (
              <p key={i} className={styles.bio}>
                {para}
              </p>
            ))}
        </div>
      </header>

      <section aria-labelledby="in-library" className={styles.section}>
        <h2 id="in-library">In your library</h2>
        {p.credits.length === 0 ? (
          <p className={styles.muted}>Nothing in the library credits them any more.</p>
        ) : (
          <ul className={styles.credits}>
            {p.credits.map((c) => (
              <CreditRow key={c.item_id} credit={c} />
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}
