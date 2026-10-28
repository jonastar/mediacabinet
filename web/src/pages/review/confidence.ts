export const pct = (c: number) => `${Math.round(c * 100)}%`
export const barColor = (c: number) =>
  c >= 0.85 ? 'var(--state-available)' : c >= 0.5 ? 'var(--accent)' : 'var(--state-missing)'
