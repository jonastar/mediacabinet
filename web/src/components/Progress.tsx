/** A thin accent progress bar along the bottom of a card or thumbnail. */
export function Progress({ value }: { value: number }) {
  return (
    <div style={{ height: 4, background: 'rgba(15, 17, 21, 0.5)' }} aria-hidden="true">
      <div
        style={{ height: 4, width: `${Math.round(value * 100)}%`, background: 'var(--accent)' }}
      />
    </div>
  )
}
