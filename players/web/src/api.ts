import type { SessionApi } from '@core/session'
import type { components, paths } from '../../../web/src/api/schema.d.ts'

type Schemas = components['schemas']
export type ClientCaps = Schemas['ClientCaps']
export type Home = Schemas['Home']
export type Episode = Schemas['Episode']
export type EpisodeList = Schemas['EpisodeList']
export type SeriesDetail = Schemas['SeriesDetail']
export type PlayPlan = Schemas['PlayPlan']
export type PairingStarted = Schemas['PairingStarted']
export type PairingStatus = Schemas['PairingStatus']
export type ServerInfo = Schemas['ServerInfo']
export type MediaToken = Schemas['MediaToken']
export type Version = Schemas['Version']

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

async function send(method: string, path: string, body?: unknown, base = server) {
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
  return res
}

type Method = 'get' | 'post' | 'put' | 'delete'
/** The methods a path has: the schema marks the others as `never`. */
type MethodOf<P extends keyof paths> = {
  [M in Method]: [NonNullable<paths[P][M]>] extends [never] ? never : M
}[Method]
type Json<T> = { content: { 'application/json': T } }

/** What an operation in the generated schema takes and gives back. */
type PathArgs<Op> = Op extends { parameters: { path: infer Params } } ? { path: Params } : unknown
type BodyArgs<Op> = Op extends { requestBody: Json<infer Body> } ? { body: Body } : unknown
type Result<Op> = Op extends { responses: { 200: Json<infer Body> } } ? Body : void
type Args<Op> = PathArgs<Op> & BodyArgs<Op> & { base?: string }

/**
 * Calls the API, with the path, parameters, body and result all checked against
 * the generated schema. Compiles away to a plain request.
 */
async function call<P extends keyof paths, M extends MethodOf<P>>(
  method: M,
  path: P,
  ...[args]: Record<string, never> extends Args<paths[P][M]>
    ? [args?: Args<paths[P][M]>]
    : [args: Args<paths[P][M]>]
): Promise<Result<paths[P][M]>> {
  const {
    path: params,
    body,
    base,
  } = (args ?? {}) as {
    path?: Record<string, unknown>
    body?: unknown
    base?: string
  }
  const url = (path as string).replace(/\{(\w+)\}/g, (_, name: string) => String(params?.[name]))
  const res = await send(method.toUpperCase(), url, body, base)
  return (res.status === 204 ? undefined : res.json()) as Promise<Result<paths[P][M]>>
}

export const api = {
  serverInfo: (address: string) => call('get', '/api/server-info', { base: address }),
  startPairing: (deviceName: string) =>
    call('post', '/api/auth/pair', { body: { device_name: deviceName } }),
  pollPairing: (secret: string) => call('post', '/api/auth/pair/poll', { body: { secret } }),
  home: () => call('get', '/api/home'),
  series: (seriesId: number) => call('get', '/api/series/{id}', { path: { id: seriesId } }),
  episodes: (seriesId: number) =>
    call('get', '/api/series/{id}/episodes', { path: { id: seriesId } }),
  versions: (itemId: number) => call('get', '/api/items/{id}/versions', { path: { id: itemId } }),
  signOut: () => call('post', '/api/auth/logout'),
  /** Subtitles as WebVTT, from the URL in a play plan. */
  subtitles: (url: string) => send('GET', url).then((res) => res.text()),
}

/** A media token that's still good for a while; for what can't send a header. */
export async function mediaToken(): Promise<string> {
  const hourFromNow = Date.now() / 1000 + 3600
  if (!media || media.expires_at < hourFromNow) {
    media = await call('post', '/api/me/media-token')
  }
  return media.token
}

/** A server URL for an `<img>` or `<video>`, with the media token in it. */
export function mediaUrl(path: string, withToken: string): string {
  const separator = path.indexOf('?') >= 0 ? '&' : '?'
  return `${server}${path}${separator}media_token=${withToken}`
}

export const sessionApi: SessionApi = {
  play: (itemId, choice, caps, startSeconds) =>
    call('post', '/api/items/{id}/play', {
      path: { id: itemId },
      body: { ...choice, caps, start_seconds: startSeconds },
    }),
  stop: (sessionId) => call('delete', '/api/play/{id}', { path: { id: sessionId } }),
  progress: (itemId, position, duration) =>
    call('put', '/api/items/{id}/progress', {
      path: { id: itemId },
      body: { position_seconds: position, duration_seconds: duration },
    }),
}
