import type { SessionApi } from '@core/session'
import type { components } from '../../../web/src/api/schema.d.ts'

type Schemas = components['schemas']
export type ClientCaps = Schemas['ClientCaps']
export type Home = Schemas['Home']
export type EpisodeList = Schemas['EpisodeList']
export type PlayPlan = Schemas['PlayPlan']
export type PairingStarted = Schemas['PairingStarted']
export type PairingStatus = Schemas['PairingStatus']
export type ServerInfo = Schemas['ServerInfo']
export type MediaToken = Schemas['MediaToken']

const SERVER_KEY = 'mc-server'
const TOKEN_KEY = 'mc-token'

function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function write(key: string, value: string | null) {
  try {
    if (value === null) {
      localStorage.removeItem(key)
    } else {
      localStorage.setItem(key, value)
    }
  } catch {
    // Without storage, this just lasts until the app closes.
  }
}

let server = read(SERVER_KEY)
let token = read(TOKEN_KEY)
let media: MediaToken | null = null
let onSignedOut = () => {}

export const session = {
  server: () => server,
  token: () => token,
  setServer(address: string | null) {
    server = address
    write(SERVER_KEY, address)
  },
  setToken(value: string | null) {
    token = value
    media = null
    write(TOKEN_KEY, value)
  },
  /** Called when the server no longer accepts the token. */
  listen(handler: () => void) {
    onSignedOut = handler
  },
}

export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

/** `tv.local:8420` as `http://tv.local:8420`, without a trailing slash. */
export function normaliseAddress(typed: string): string {
  const trimmed = typed.trim().replace(/\/+$/, '')
  return /^https?:\/\//i.test(trimmed) ? trimmed : `http://${trimmed}`
}

async function request<T>(method: string, path: string, body?: unknown, base = server): Promise<T> {
  const headers: Record<string, string> = {}
  if (body !== undefined) {
    headers['content-type'] = 'application/json'
  }
  if (token) {
    headers.authorization = `Bearer ${token}`
  }
  const res = await fetch(`${base}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (res.status === 401 && token) {
    session.setToken(null)
    onSignedOut()
  }
  if (!res.ok) {
    const detail = await res.json().catch(() => null)
    throw new ApiError(res.status, detail?.error ?? `${method} ${path}: ${res.status}`)
  }
  if (res.status === 204) {
    return undefined as T
  }
  return res.json() as Promise<T>
}

export const api = {
  serverInfo: (address: string) =>
    request<ServerInfo>('GET', '/api/server-info', undefined, address),
  startPairing: (deviceName: string) =>
    request<PairingStarted>('POST', '/api/auth/pair', { device_name: deviceName }),
  pollPairing: (secret: string) =>
    request<PairingStatus>('POST', '/api/auth/pair/poll', { secret }),
  home: () => request<Home>('GET', '/api/home'),
  episodes: (seriesId: number) => request<EpisodeList>('GET', `/api/series/${seriesId}/episodes`),
  signOut: () => request<void>('POST', '/api/auth/logout'),
}

/** A media token that's still good for a while; for what can't send a header. */
export async function mediaToken(): Promise<string> {
  const hourFromNow = Date.now() / 1000 + 3600
  if (!media || media.expires_at < hourFromNow) {
    media = await request<MediaToken>('POST', '/api/me/media-token')
  }
  return media.token
}

/** A server URL for an `<img>` or `<video>`, with the media token in it. */
export function mediaUrl(path: string, withToken: string): string {
  const separator = path.indexOf('?') >= 0 ? '&' : '?'
  return `${server}${path}${separator}media_token=${withToken}`
}

export const sessionApi: SessionApi = {
  play: (itemId, caps, startSeconds) =>
    request<PlayPlan>('POST', `/api/items/${itemId}/play`, { caps, start_seconds: startSeconds }),
  stop: (sessionId) => request<void>('DELETE', `/api/play/${sessionId}`),
  progress: (itemId, position, duration) =>
    request('PUT', `/api/items/${itemId}/progress`, {
      position_seconds: position,
      duration_seconds: duration,
    }),
}
