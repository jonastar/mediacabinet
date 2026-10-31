import { useId } from 'react'
import markUrl from '../assets/mark.svg'
import styles from './Brand.module.css'

/** The mark and wordmark, as in docs/brand/lockup. The wordmark is live text in Manrope. */
export function Brand() {
  return (
    <>
      <img src={markUrl} alt="" width={22} height={25} className={styles.mark} />
      <span className={styles.wordmark}>
        media <b>cabinet</b>
      </span>
    </>
  )
}

const prefersReducedMotion = () => window.matchMedia?.('(prefers-reduced-motion: reduce)').matches

/** The looping light-leak from docs/brand/motion, or the still mark when motion is reduced. */
export function LoadingMark({ size = 64 }: { size?: number }) {
  const clip = useId()
  if (prefersReducedMotion()) {
    // The still mark is cropped to its shape (44×50 of the 64×64 box).
    const [width, height] = [(size * 44) / 64, (size * 50) / 64]
    return <img src={markUrl} alt="Loading" width={width} height={height} className={styles.mark} />
  }
  const ease = { dur: '2.4s', repeatCount: 'indefinite', keyTimes: '0;0.5;1', calcMode: 'spline' }
  const spline = '0.25 0 0.25 1;0.25 0 0.25 1'
  return (
    <svg viewBox="0 0 64 64" width={size} height={size} role="img" aria-label="Loading">
      <defs>
        <clipPath id={clip}>
          <rect x="10" y="7" width="44" height="50" rx="6" />
        </clipPath>
      </defs>
      <g clipPath={`url(#${clip})`}>
        <rect x="10" y="7" width="44" height="50" fill="var(--brand-play)" opacity="0.25">
          <animate attributeName="opacity" values="0.25;1;0.25" keySplines={spline} {...ease} />
        </rect>
        <g>
          <animateTransform
            attributeName="transform"
            type="translate"
            values="0 0;-2.5 0;0 0"
            keySplines={spline}
            {...ease}
          />
          <path
            fill="var(--brand-cabinet)"
            d="M16 7 H30.5 V57 H16 A6 6 0 0 1 10 51 V13 A6 6 0 0 1 16 7 Z"
          />
        </g>
        <g>
          <animateTransform
            attributeName="transform"
            type="translate"
            values="0 0;2.5 0;0 0"
            keySplines={spline}
            {...ease}
          />
          <path
            fill="var(--brand-cabinet)"
            d="M33.5 7 H48 A6 6 0 0 1 54 13 V51 A6 6 0 0 1 48 57 H33.5 Z"
          />
        </g>
      </g>
      {/* The play symbol's outline is drawn in the page colour, since it moves apart from the doors. */}
      <path
        fill="var(--brand-play)"
        stroke="var(--bg)"
        strokeWidth="6"
        strokeLinejoin="round"
        paintOrder="stroke"
        d="M25 21 L43 32 L25 43 Z"
      />
    </svg>
  )
}

/** Shown while the app checks the session on start. */
export function BootScreen() {
  return (
    <main className={styles.boot}>
      <LoadingMark />
    </main>
  )
}
