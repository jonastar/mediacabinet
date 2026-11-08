import { useSyncExternalStore } from 'react'
import { api, SIGNED_OUT_EVENT, type ProviderButton, type User } from '../api/client'

/** What the login page offers. */
export interface SignInOptions {
  providers: ProviderButton[]
  /** Show the password form up front. */
  passwordSignIn: boolean
  /** Offer "Continue as guest". */
  guestAccess: boolean
}

/**
 * Who is signed in. `checking` until the server has answered; `setup` on a
 * fresh install; `signedOut` shows the login screen; `pending` is signed in
 * with another account and waiting for an admin.
 */
export type Session =
  | { state: 'checking' }
  | { state: 'setup' }
  | { state: 'signedOut'; options: SignInOptions }
  | { state: 'pending'; user: User }
  | { state: 'signedIn'; user: User }
  | { state: 'unreachable'; error: Error }

let current: Session = { state: 'checking' }
let options: SignInOptions = { providers: [], passwordSignIn: true, guestAccess: false }
/** Signed out on purpose: don't send them straight back to a provider. */
let signedOutHere = false
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
    options = {
      providers: status.providers,
      passwordSignIn: status.password_sign_in,
      guestAccess: status.guest_access,
    }
    if (status.setup_needed) {
      set({ state: 'setup' })
    } else if (status.user) {
      signedIn(status.user)
    } else {
      set({ state: 'signedOut', options })
    }
  } catch (error) {
    set({ state: 'unreachable', error: error as Error })
  }
}

export function signedIn(user: User) {
  set(user.status === 'pending' ? { state: 'pending', user } : { state: 'signedIn', user })
}

export async function signOut() {
  await api.logout().catch(() => undefined)
  signedOutHere = true
  set({ state: 'signedOut', options })
}

/** Whether this tab signed out on purpose (so the login page shouldn't sign straight back in). */
export function signedOutOnPurpose() {
  return signedOutHere
}

window.addEventListener(SIGNED_OUT_EVENT, () => {
  if (current.state === 'signedIn' || current.state === 'pending') {
    set({ state: 'signedOut', options })
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
