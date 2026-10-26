/** Line icons from the mockups. Decorative: always aria-hidden. */
const line = {
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1.7,
  'aria-hidden': true,
} as const

export const LogoIcon = () => (
  <svg
    width="26"
    height="26"
    viewBox="0 0 26 26"
    fill="none"
    stroke="var(--accent)"
    strokeWidth="2"
    aria-hidden="true"
  >
    <rect x="4" y="3" width="18" height="20" rx="2.5" />
    <path d="M4 10h18M4 16.5h18M11 6.5h4M11 13h4M11 19.5h4" />
  </svg>
)

export const HomeIcon = () => (
  <svg width="18" height="18" viewBox="0 0 20 20" {...line}>
    <path d="M3 9.5L10 3.5l7 6V17h-5v-4.5H8V17H3z" />
  </svg>
)

export const MoviesIcon = () => (
  <svg width="18" height="18" viewBox="0 0 20 20" {...line}>
    <rect x="3" y="3" width="14" height="14" rx="2" />
    <path d="M7 3v14M13 3v14M3 7h4M3 13h4M13 7h4M13 13h4" />
  </svg>
)

export const TvIcon = () => (
  <svg width="18" height="18" viewBox="0 0 20 20" {...line}>
    <rect x="2.5" y="5" width="15" height="10.5" rx="2" />
    <path d="M7 2.5l3 2.5 3-2.5M7 18h6" />
  </svg>
)

export const CalendarIcon = () => (
  <svg width="18" height="18" viewBox="0 0 20 20" {...line}>
    <rect x="3" y="4" width="14" height="13" rx="2" />
    <path d="M3 8h14M7 2.5v3M13 2.5v3" />
  </svg>
)

export const WantedIcon = () => (
  <svg width="18" height="18" viewBox="0 0 20 20" {...line}>
    <circle cx="10" cy="10" r="7" />
    <circle cx="10" cy="10" r="3" />
  </svg>
)

export const ReviewIcon = () => (
  <svg width="18" height="18" viewBox="0 0 20 20" {...line}>
    <path d="M4 10.5l3.5 3.5L16 5.5" />
  </svg>
)

export const FolderIcon = () => (
  <svg width="18" height="18" viewBox="0 0 20 20" {...line}>
    <path d="M2.5 6V15.5h15V7.5H9.5L8 5.5H3z" />
  </svg>
)

export const BackIcon = () => (
  <svg
    width="16"
    height="16"
    viewBox="0 0 20 20"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    aria-hidden="true"
  >
    <path d="M12.5 4L6.5 10l6 6" />
  </svg>
)

export const PlayIcon = () => (
  <svg width="13" height="13" viewBox="0 0 14 14" aria-hidden="true">
    <path d="M3 1.5l9 5.5-9 5.5z" fill="currentColor" />
  </svg>
)

export const ListIcon = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" {...line}>
    <path d="M2 3.5h12M2 8h12M2 12.5h12" />
  </svg>
)

export const GridIcon = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" {...line}>
    <rect x="2" y="2" width="5" height="5" rx="1" />
    <rect x="9" y="2" width="5" height="5" rx="1" />
    <rect x="2" y="9" width="5" height="5" rx="1" />
    <rect x="9" y="9" width="5" height="5" rx="1" />
  </svg>
)

export const MoreIcon = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
    <circle cx="3" cy="8" r="1.5" />
    <circle cx="8" cy="8" r="1.5" />
    <circle cx="13" cy="8" r="1.5" />
  </svg>
)

export const SettingsIcon = () => (
  <svg width="18" height="18" viewBox="0 0 20 20" {...line}>
    <circle cx="10" cy="10" r="2.6" />
    <path d="M10 2.5v2.2M10 15.3v2.2M2.5 10h2.2M15.3 10h2.2M4.7 4.7l1.6 1.6M13.7 13.7l1.6 1.6M4.7 15.3l1.6-1.6M13.7 6.3l1.6-1.6" />
  </svg>
)
