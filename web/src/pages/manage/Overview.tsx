import { api, type Home } from '../../api/client'
import { useApi } from '../../api/useApi'
import { Link } from '../../components/Link'
import { Failed, Loading } from '../../components/Status'
import { useLiveEvents } from '../../lib/live'
import styles from './Manage.module.css'

/** The Manage landing tab: what needs an admin, each linking to where to act on it. */
export function Overview() {
  const home = useApi('home', api.home)
  useLiveEvents(
    [
      'file.added',
      'file.missing',
      'item.wanted',
      'item.unwanted',
      'match.needs_review',
      'status.changed',
      'scan.finished',
    ],
    home.reload,
    1000,
  )
  if (home.error) {
    return <Failed error={home.error} onRetry={home.reload} />
  }
  if (!home.data) {
    return <Loading />
  }
  return (
    <div className={styles.page}>
      <section aria-labelledby="manage-attention" className={styles.section}>
        <h1 id="manage-attention">Needs attention</h1>
        <Cards home={home.data} />
      </section>
    </div>
  )
}

function Cards({ home }: { home: Home }) {
  const a = home.attention
  const cards = [
    {
      to: '/manage/review',
      value: a.review,
      color: 'var(--state-attention)',
      label: 'Matches to review',
      note: 'Below the confidence threshold',
    },
    {
      to: '/manage/libraries',
      value: a.missing_files,
      color: 'var(--state-missing)',
      label: 'Files missing',
      note: a.offline_roots.length
        ? `${a.offline_roots.join(', ')} offline · nothing removed`
        : 'Not found on the last scan',
    },
    {
      to: '/manage/wanted',
      value: a.wanted,
      label: 'Wanted',
      note: a.in_progress
        ? `${a.in_progress} in progress via ${a.status_sources.join(', ')}`
        : 'Monitored and not in the library',
    },
    { to: '/tv', value: a.gaps, label: 'Gaps', note: `In ${a.gap_series} series` },
  ]
  return (
    <div className={styles.attention}>
      {cards.map((c) => (
        <Link key={c.label} to={c.to} className={styles.attentionCard}>
          <span className={styles.attentionValue} style={{ color: c.color }}>
            {c.value}
          </span>
          <span className={styles.attentionLabel}>{c.label}</span>
          <span className={styles.attentionNote}>{c.note}</span>
        </Link>
      ))}
    </div>
  )
}
