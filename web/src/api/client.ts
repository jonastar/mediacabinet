import type { components } from './schema'

type Schemas = components['schemas']
export type Health = Schemas['Health']
export type Home = Schemas['Home']
export type SeriesSummary = Schemas['SeriesSummary']
export type SeriesDetail = Schemas['SeriesDetail']
export type EpisodeList = Schemas['EpisodeList']
export type Episode = Schemas['Episode']
export type EpisodeRef = Schemas['EpisodeRef']
export type EpisodeNumber = Schemas['EpisodeNumber']
export type ContinueItem = Schemas['ContinueItem']
export type LinkedFile = Schemas['LinkedFile']
export type ExternalStatus = Schemas['ExternalStatus']
export type View = Schemas['View']
export type Availability = Schemas['Availability']

export class ApiError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
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
  health: () => request<Health>('GET', '/api/health'),
  home: () => request<Home>('GET', '/api/home'),
  series: () => request<SeriesSummary[]>('GET', '/api/series'),
  seriesDetail: (id: number) => request<SeriesDetail>('GET', `/api/series/${id}`),
  episodes: (id: number, view?: string | null) =>
    request<EpisodeList>(
      'GET',
      `/api/series/${id}/episodes${view ? `?view=${encodeURIComponent(view)}` : ''}`,
    ),
  setView: (id: number, view: string) => request<void>('PUT', `/api/series/${id}/view`, { view }),
  setMonitored: (itemId: number, monitored: boolean | null) =>
    request<void>('PUT', `/api/items/${itemId}/monitor`, { monitored }),
}
