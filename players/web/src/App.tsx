import { useCallback, useEffect, useState } from 'preact/hooks'
import { hosted, session } from './api'
import { currentPlace, returnTo, screenHandledBack, useRemote, type Place } from './focus'
import { Browse } from './screens/Browse'
import { Connect } from './screens/Connect'
import { Diagnostics } from './screens/Diagnostics'
import { Pair } from './screens/Pair'
import { Player } from './screens/Player'
import { Series } from './screens/Series'

export type Screen =
  | { name: 'browse' }
  | { name: 'series'; seriesId: number; title: string }
  | { name: 'player'; itemId: number; title: string }
  | { name: 'diagnostics' }

/** A screen in the stack, and where focus was when the next one opened over it. */
type Entry = Screen & { place?: Place | null }

/**
 * Connect, then pair, then a stack of screens, with Back going down it. Hosted by
 * the server, it starts at the stack: the web UI has already signed in.
 */
export function App() {
  const [server, setServer] = useState(session.server())
  const [token, setToken] = useState(session.token())
  const [stack, setStack] = useState<Entry[]>([{ name: 'browse' }])

  useEffect(() => {
    session.listen(() => {
      if (hosted) {
        // Signing in happens in the web UI, which hosts this app.
        location.replace('/')
      } else {
        setToken(null)
      }
    })
  }, [])

  const open = useCallback((screen: Screen) => {
    const place = currentPlace()
    returnTo(null)
    setStack((s) => [...s.slice(0, -1), { ...s[s.length - 1], place }, screen])
  }, [])
  const replace = useCallback((screen: Screen) => {
    setStack((s) => {
      const top = s[s.length - 1]
      const below = s[s.length - 2]
      // Moving on to the next episode: coming back lands on that one, not where it began.
      if (top.name === 'player' && screen.name === 'player' && below?.place) {
        const place =
          below.place.key === `episode-${top.itemId}`
            ? { ...below.place, key: `episode-${screen.itemId}` }
            : below.place
        return [...s.slice(0, -2), { ...below, place }, screen]
      }
      return [...s.slice(0, -1), screen]
    })
  }, [])
  const back = useCallback(() => {
    if (stack.length > 1) {
      returnTo(stack[stack.length - 2].place)
      setStack(stack.slice(0, -1))
    }
  }, [stack])
  const onRemote = useCallback(
    (action: string) => {
      if (action === 'back' && !screenHandledBack()) {
        back()
      }
    },
    [back],
  )
  useRemote(onRemote)

  if (!server) {
    return (
      <Connect
        onConnected={(address) => {
          session.setServer(address)
          setServer(address)
        }}
      />
    )
  }
  const changeServer = () => {
    if (hosted) {
      location.assign('/')
      return
    }
    session.setToken(null)
    session.setServer(null)
    setToken(null)
    setServer(null)
    setStack([{ name: 'browse' }])
  }
  if (!token && !hosted) {
    return (
      <Pair
        server={server}
        onPaired={(newToken) => {
          session.setToken(newToken)
          setToken(newToken)
          setStack([{ name: 'browse' }])
        }}
        onChangeServer={changeServer}
      />
    )
  }
  const screen = stack[stack.length - 1]
  switch (screen.name) {
    case 'series':
      return <Series key={screen.seriesId} screen={screen} open={open} />
    case 'player':
      return <Player key={screen.itemId} screen={screen} onLeave={back} onReplace={replace} />
    case 'diagnostics':
      return <Diagnostics />
    default:
      return <Browse open={open} onChangeServer={changeServer} />
  }
}
