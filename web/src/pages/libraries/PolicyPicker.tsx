import type { WritePolicy } from '../../api/client'
import { POLICIES, POLICY_NOTES } from './roots'
import styles from './Libraries.module.css'

interface Props {
  name: string
  value: WritePolicy
  disabled?: boolean
  onChange: (policy: WritePolicy) => void
}

/** The write policies side by side, each with what it does, so you can read before choosing. */
export function PolicyPicker({ name, value, disabled, onChange }: Props) {
  return (
    <div role="radiogroup" aria-label="Write policy" className={styles.policyOptions}>
      {POLICIES.map(([policy, label]) => (
        <label key={policy} className={styles.policyOption}>
          <input
            type="radio"
            name={name}
            value={policy}
            checked={value === policy}
            disabled={disabled}
            onChange={() => onChange(policy)}
          />
          <span className={styles.policyText}>
            <b>{label}</b>
            <span>{POLICY_NOTES[policy]}</span>
          </span>
        </label>
      ))}
    </div>
  )
}
