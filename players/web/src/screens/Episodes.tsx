import { useEffect, useState } from 'preact/hooks'
import type { Screen } from '../App'
import { api, type EpisodeList } from '../api'
import { useInitialFocus } from '../focus'

function label(number: { season?: number | null; episode?: number | null } | null | undefined) {
  if (!number) {
    return ''
  }
  return `S${number.season ?? '?'} E${number.episode ?? '?'} · `
}

/** A show's episodes that can be played. */
export function Episodes({
  screen,
  open,
}: {
  screen: { seriesId: number; title: string }
  open: (screen: Screen) => void
}) {
  const [list, setList] = useState<EpisodeList | null>(null)
  const [problem, setProblem] = useState<string | null>(null)

  useEffect(() => {
    let current = true
    api.episodes(screen.seriesId).then(
      (loaded) => {
        if (current) {
          setList(loaded)
        }
      },
      (err: Error) => {
        if (current) {
          setProblem(err.message)
        }
      },
    )
    return () => {
      current = false
    }
  }, [screen.seriesId])

  const playable = list ? list.entries.filter((entry) => entry.files.length > 0) : []
  const upNext = list?.up_next
  useInitialFocus(list)

  return (
    <main className="browse">
      <h1>{screen.title}</h1>
      {problem && (
        <p className="problem" role="alert">
          {problem}
        </p>
      )}
      {list && playable.length === 0 && <p className="muted">No episodes to play yet.</p>}
      <div className="list">
        {playable.map((entry) => (
          <button
            key={entry.item_id}
            className="row"
            data-first={entry.item_id === upNext ? '' : undefined}
            onClick={() =>
              open({
                name: 'player',
                itemId: entry.item_id,
                title: `${screen.title}: ${entry.title}`,
              })
            }
          >
            {label(entry.number)}
            {entry.title}
          </button>
        ))}
      </div>
    </main>
  )
}
