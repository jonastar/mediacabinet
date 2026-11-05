/** Where a code waits while someone signs in with a provider, which leaves the page. */
const KEY = 'mc-pair-code'

/** Keeps the code from a `/pair` link across signing in. */
export function holdPairCode() {
  if (window.location.pathname !== '/pair') {
    return
  }
  const code = new URLSearchParams(window.location.search).get('code')
  if (code) {
    try {
      sessionStorage.setItem(KEY, code)
    } catch {
      // Without storage, the person types the code again.
    }
  }
}

/** The address to carry on at after signing in, if a `/pair` link was interrupted. */
export function takeHeldPairPath(): string | null {
  try {
    const code = sessionStorage.getItem(KEY)
    sessionStorage.removeItem(KEY)
    return code ? `/pair?code=${encodeURIComponent(code)}` : null
  } catch {
    return null
  }
}
