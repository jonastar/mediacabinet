import type { Badge as BadgeValue } from '../lib/wording'
import styles from './Badge.module.css'

export function Badge({ badge, small }: { badge: BadgeValue; small?: boolean }) {
  return (
    <span className={`${styles.badge} ${styles[badge.tone]} ${small ? styles.small : ''}`}>
      {badge.label}
    </span>
  )
}
