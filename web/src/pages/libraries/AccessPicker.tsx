import { ACCESS } from './roots'
import styles from './Libraries.module.css'

interface Props {
  name: string
  readOnly: boolean
  disabled?: boolean
  onChange: (readOnly: boolean) => void
}

/** Writable or read-only, side by side, each with what it means, so you can read before choosing. */
export function AccessPicker({ name, readOnly, disabled, onChange }: Props) {
  return (
    <div role="radiogroup" aria-label="Writing" className={styles.policyOptions}>
      {ACCESS.map(({ readOnly: value, label, note }) => (
        <label key={label} className={styles.policyOption}>
          <input
            type="radio"
            name={name}
            value={label}
            checked={readOnly === value}
            disabled={disabled}
            onChange={() => onChange(value)}
          />
          <span className={styles.policyText}>
            <b>{label}</b>
            <span>{note}</span>
          </span>
        </label>
      ))}
    </div>
  )
}
