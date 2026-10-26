import { useSyncExternalStore } from 'react'
import { api, SIGNED_OUT_EVENT, type User } from '../api/client'

/**
 * Who is signed in. `checking` until the server has answered; `setup` on a
 * fresh install; `signedOut` shows the login screen.
 */
export type Session =
  | { state: 'checking' }
  | { state: 'setup' }
  | { state: 'signedOut' }
  | { state: 'signedIn'; user: User }
  | { state: 'unreachable'; error: Error }

let current: Session = { state: 'checking' }
const listeners = new Set<() => void>()

function set(next: Session) {
  current = next
  listeners.forEach((l) => l())
}

function subscribe(onChange: () => void) {
  listeners.add(onChange)
  return () => listeners.delete(onChange)
}

export async function checkSession() {
  try {
    const status = await api.authStatus()
    if (status.setup_needed) {
      set({ state: 'setup' })
    } else if (status.user) {
      set({ state: 'signedIn', user: status.user })
    } else {
      set({ state: 'signedOut' })
    }
  } catch (error) {
    set({ state: 'unreachable', error: error as Error })
  }
}

export function signedIn(user: User) {
  set({ state: 'signedIn', user })
}

export async function signOut() {
  await api.logout().catch(() => undefined)
  set({ state: 'signedOut' })
}

window.addEventListener(SIGNED_OUT_EVENT, () => {
  if (current.state === 'signedIn') {
    set({ state: 'signedOut' })
  }
})

export function useSession(): Session {
  return useSyncExternalStore(subscribe, () => current)
}

/** The signed-in user. Only use below the sign-in gate. */
export function useUser(): User {
  const session = useSession()
  if (session.state !== 'signedIn') {
    throw new Error('useUser needs a signed-in user')
  }
  return session.user
}
