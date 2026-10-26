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
export type Library = Schemas['Library']
export type Root = Schemas['Root']
export type TemplatePiece = Schemas['TemplatePiece']
export type FolderProposal = Schemas['FolderProposal']
export type TemplateCheck = Schemas['TemplateCheck']
export type FolderListing = Schemas['FolderListing']
export type PlacementExample = Schemas['PlacementExample']
export type ScanProgress = Schemas['ScanProgress']
export type ReviewQueue = Schemas['ReviewQueue']
export type FolderReview = Schemas['FolderReview']
export type FileReview = Schemas['FileReview']
export type Candidate = Schemas['Candidate']
export type SearchResult = Schemas['SearchResult']
export type PathPiece = Schemas['PathPiece']
export type MovieSummary = Schemas['MovieSummary']
export type MovieDetail = Schemas['MovieDetail']
export type SearchResults = Schemas['SearchResults']
export type ImportReport = Schemas['ImportReport']
export type WantedPage = Schemas['WantedPage']
export type WantedItem = Schemas['WantedItem']
export type CalendarEntry = Schemas['CalendarEntry']
export type QualityProfile = Schemas['QualityProfile']
export type ApiKey = Schemas['ApiKey']
export type NewApiKey = Schemas['NewApiKey']
export type Webhook = Schemas['Webhook']
export type NewWebhook = Schemas['NewWebhook']
export type MovePlan = Schemas['MovePlan']
export type LibraryKind = 'tv' | 'movies'

export interface WebhookInput {
  name?: string
  url?: string
  event_types?: string[]
  enabled?: boolean
  from_start?: boolean
}

export interface WantedQuery {
  reason?: 'missing' | 'not_acquired' | 'upgrade'
  series_id?: number
  library_id?: number
  limit?: number
  offset?: number
}

export interface ItemEdit {
  title?: string
  year?: number | null
  overview?: string
  lock?: boolean
}
export type WritePolicy = 'managed' | 'adaptive' | 'read_only'

export interface NewRoot {
  path: string
  write_policy: WritePolicy
  layout_template: string
  is_import_target: boolean
}

export interface RootChanges {
  write_policy?: WritePolicy
  layout_template?: string
  is_import_target?: boolean
}

export class ApiError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

/** The URL of an item's artwork, if it has that kind. */
export function artworkUrl(item: { id: number; artwork: string[] }, kind: 'poster' | 'backdrop') {
  return item.artwork.includes(kind) ? `/api/artwork/${item.id}/${kind}` : null
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

  libraries: () => request<Library[]>('GET', '/api/libraries'),
  createLibrary: (name: string, kind: LibraryKind) =>
    request<Library>('POST', '/api/libraries', { name, kind }),
  deleteLibrary: (id: number) => request<void>('DELETE', `/api/libraries/${id}`),
  addRoot: (libraryId: number, root: NewRoot) =>
    request<Root>('POST', `/api/libraries/${libraryId}/roots`, root),
  updateRoot: (id: number, changes: RootChanges) =>
    request<Root>('PATCH', `/api/roots/${id}`, changes),
  removeRoot: (id: number) => request<void>('DELETE', `/api/roots/${id}`),
  scanRoot: (id: number) => request<{ started: boolean }>('POST', `/api/roots/${id}/scan`),
  propose: (path: string, kind: LibraryKind) =>
    request<FolderProposal>('POST', '/api/folders/propose', { path, kind }),
  checkTemplate: (template: string, kind: LibraryKind) =>
    request<TemplateCheck>('POST', '/api/templates/check', { template, kind }),
  folders: (path: string) =>
    request<FolderListing>('GET', `/api/folders?path=${encodeURIComponent(path)}`),
  review: () => request<ReviewQueue>('GET', '/api/review'),
  acceptCandidate: (id: number, pin: boolean) =>
    request<void>('POST', `/api/review/candidates/${id}/accept`, { pin }),
  rejectCandidate: (id: number) => request<void>('POST', `/api/review/candidates/${id}/reject`),
  linkFile: (fileId: number, itemIds: number[], pin: boolean) =>
    request<void>('POST', `/api/files/${fileId}/link`, { item_ids: itemIds, pin }),
  ignoreFile: (fileId: number) => request<void>('POST', `/api/files/${fileId}/ignore`),
  identifyFolder: (rootId: number, folderPath: string, providerId: string) =>
    request<void>('POST', '/api/review/folders/identify', {
      root_id: rootId,
      folder_path: folderPath,
      provider_id: providerId,
    }),
  searchMetadata: (kind: LibraryKind, query: string, year?: number | null) =>
    request<SearchResult[]>(
      'GET',
      `/api/metadata/search?kind=${kind}&query=${encodeURIComponent(query)}${year ? `&year=${year}` : ''}`,
    ),
  movies: () => request<MovieSummary[]>('GET', '/api/movies'),
  movieDetail: (id: number) => request<MovieDetail>('GET', `/api/movies/${id}`),
  search: (q: string) => request<SearchResults>('GET', `/api/search?q=${encodeURIComponent(q)}`),
  editItem: (id: number, edit: ItemEdit) => request<void>('PATCH', `/api/items/${id}`, edit),
  setLocks: (id: number, fields: string[]) =>
    request<void>('PUT', `/api/items/${id}/locks`, { fields }),
  refreshItem: (id: number) => request<ImportReport>('POST', `/api/items/${id}/refresh`),
  setMatchOrdering: (seriesId: number, view: string | null) =>
    request<void>('PUT', `/api/series/${seriesId}/match-ordering`, { view }),
  rematchSeries: (seriesId: number, providerId: string) =>
    request<ImportReport>('POST', `/api/series/${seriesId}/rematch`, { provider_id: providerId }),
  rescanSeries: (seriesId: number) => request<void>('POST', `/api/series/${seriesId}/rescan`),
  wanted: (q: WantedQuery = {}) => {
    const params = new URLSearchParams()
    for (const [k, v] of Object.entries(q)) {
      if (v !== undefined) {
        params.set(k, String(v))
      }
    }
    return request<WantedPage>('GET', `/api/wanted?${params}`)
  },
  calendar: (from: number, to: number) =>
    request<CalendarEntry[]>('GET', `/api/calendar?from=${from}&to=${to}`),
  qualityProfiles: () => request<QualityProfile[]>('GET', '/api/quality-profiles'),
  setMonitoring: (
    itemId: number,
    settings: { quality_profile_id?: number | null; wanted_follows?: string | null },
  ) => request<void>('PUT', `/api/items/${itemId}/monitoring`, settings),
  apiKeys: () => request<ApiKey[]>('GET', '/api/api-keys'),
  createApiKey: (name: string) => request<NewApiKey>('POST', '/api/api-keys', { name }),
  revokeApiKey: (id: number) => request<void>('DELETE', `/api/api-keys/${id}`),
  webhooks: () => request<Webhook[]>('GET', '/api/webhooks'),
  createWebhook: (input: WebhookInput) => request<NewWebhook>('POST', '/api/webhooks', input),
  updateWebhook: (id: number, input: WebhookInput) =>
    request<Webhook>('PATCH', `/api/webhooks/${id}`, input),
  deleteWebhook: (id: number) => request<void>('DELETE', `/api/webhooks/${id}`),
  testWebhook: (id: number) =>
    request<{ ok: boolean; error?: string | null }>('POST', `/api/webhooks/${id}/test`),
  organize: (itemId: number, dryRun: boolean) =>
    request<MovePlan>('POST', `/api/items/${itemId}/organize`, { dry_run: dryRun }),
  moveSeries: (seriesId: number, rootId: number, dryRun: boolean) =>
    request<MovePlan>('POST', `/api/series/${seriesId}/move`, { root_id: rootId, dry_run: dryRun }),
  placementExample: (libraryId: number) =>
    request<PlacementExample | null>('GET', `/api/libraries/${libraryId}/placement-example`),
}
