import { useCallback, useEffect, useState } from 'preact/hooks'
import { session } from './api'
import { screenHandledBack, useRemote } from './focus'
import { Browse } from './screens/Browse'
import { Connect } from './screens/Connect'
import { Diagnostics } from './screens/Diagnostics'
import { Episodes } from './screens/Episodes'
import { Pair } from './screens/Pair'
import { Player } from './screens/Player'

export type Screen =
  | { name: 'browse' }
  | { name: 'episodes'; seriesId: number; title: string }
  | { name: 'player'; itemId: number; title: string }
  | { name: 'diagnostics' }

/** Connect, then pair, then a stack of screens, with Back going down it. */
export function App() {
  const [server, setServer] = useState(session.server())
  const [token, setToken] = useState(session.token())
  const [stack, setStack] = useState<Screen[]>([{ name: 'browse' }])

  useEffect(() => {
    session.listen(() => setToken(null))
  }, [])

  const open = useCallback((screen: Screen) => setStack((s) => [...s, screen]), [])
  const replace = useCallback((screen: Screen) => setStack((s) => [...s.slice(0, -1), screen]), [])
  const back = useCallback(() => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s)), [])
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
    session.setToken(null)
    session.setServer(null)
    setToken(null)
    setServer(null)
    setStack([{ name: 'browse' }])
  }
  if (!token) {
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
    case 'episodes':
      return <Episodes key={screen.seriesId} screen={screen} open={open} />
    case 'player':
      return <Player key={screen.itemId} screen={screen} onLeave={back} onReplace={replace} />
    case 'diagnostics':
      return <Diagnostics />
    default:
      return <Browse open={open} onChangeServer={changeServer} />
  }
}
