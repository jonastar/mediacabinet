/**
 * Which app this build is, and its version. `players/tizen` and `players/webos`
 * set them when they package it. Hosted at `/tv-mode/`, the server is the app's
 * own, and in `pnpm dev` there's no version to check.
 */
export const APP_PLATFORM: 'tizen' | 'webos' | undefined = import.meta.env.VITE_APP_PLATFORM
export const APP_VERSION: string | undefined = import.meta.env.VITE_APP_VERSION

/**
 * The oldest server the packaged apps work with, checked against `version` in
 * `GET /api/server-info`. Raise it when the app starts using something newer
 * servers added (see docs/releasing.md).
 */
export const MIN_SERVER_VERSION = '2026.1.0'

/**
 * Whether `version` is at least `minimum`, comparing `year.release.patch` and
 * ignoring anything after a `-`, so a server working toward 2026.2.0 counts as it.
 */
export function meetsMinimum(version: string, minimum: string) {
  const parse = (v: string) =>
    v
      .split('-')[0]
      .split('.')
      .map((n) => parseInt(n, 10) || 0)
  const a = parse(version)
  const b = parse(minimum)
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if ((a[i] || 0) !== (b[i] || 0)) {
      return (a[i] || 0) > (b[i] || 0)
    }
  }
  return true
}
