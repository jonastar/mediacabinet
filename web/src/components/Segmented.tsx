import type { ReactNode } from 'react'
import styles from './Segmented.module.css'

export interface Option<T extends string> {
  value: T
  label: string
  icon?: ReactNode
}

interface Props<T extends string> {
  label: string
  options: Option<T>[]
  value: T | null | undefined
  onChange: (value: T) => void
  variant: 'pills' | 'box'
}

/** A row of `aria-pressed` buttons; the selected one is filled. */
export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  variant,
}: Props<T>) {
  return (
    <div role="group" aria-label={label} className={`${styles.group} ${styles[variant]}`}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          className={styles.option}
          aria-pressed={o.value === value}
          onClick={() => onChange(o.value)}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  )
}
