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
export type User = Schemas['User']
export type AuthStatus = Schemas['AuthStatus']
export type ItemWatch = Schemas['ItemWatch']
export type Playlist = Schemas['Playlist']
export type PlaylistInput = Schemas['PlaylistInput']
export type Version = Schemas['Version']
export type PlayPlan = Schemas['PlayPlan']
export type ClientCaps = Schemas['ClientCaps']
export type PlayContext = Schemas['PlayContext']
export type ItemView = Schemas['ItemView']
export type OrderingNumber = Schemas['OrderingNumber']
export type EventRecord = Schemas['EventRecord']
export type ActivityEntry = Schemas['ActivityEntry']
export type ActivityPage = Schemas['ActivityPage']
export type LibraryKind = 'tv' | 'movies'

export interface PlayChoice {
  file_id?: number
  audio_stream?: number
  subtitle?: string | null
  max_height?: number | null
}

export interface WebhookInput {
  name?: string
  url?: string
  event_types?: string[]
  enabled?: boolean
  from_start?: boolean
}

export interface ActivityQuery {
  kinds?: string
  library_id?: number
  item_id?: number
  user_id?: number
  before?: string
  limit?: number
}

function query(q: object) {
  const params = new URLSearchParams()
  for (const [k, v] of Object.entries(q)) {
    if (v !== undefined) {
      params.set(k, String(v))
    }
  }
  return params.toString()
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

/** Fired when the session has ended (signed out elsewhere, expired). */
export const SIGNED_OUT_EVENT = 'mc:signed-out'

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
  init: RequestInit = {},
): Promise<T> {
  const res = await fetch(path, {
    method,
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    ...init,
  })
  if (res.status === 401 && !path.startsWith('/api/auth/')) {
    window.dispatchEvent(new Event(SIGNED_OUT_EVENT))
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
  health: () => request<Health>('GET', '/api/health'),
  home: () => request<Home>('GET', '/api/home'),
  series: (library?: number) =>
    request<SeriesSummary[]>('GET', `/api/series${library ? `?library=${library}` : ''}`),
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
  renameLibrary: (id: number, name: string) =>
    request<Library>('PATCH', `/api/libraries/${id}`, { name }),
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
  /** The file isn't that item; scans won't link them again. */
  unlinkFile: (fileId: number, itemId: number) =>
    request<void>('DELETE', `/api/files/${fileId}/links/${itemId}`),
  setEdition: (fileId: number, itemId: number, edition: string | null) =>
    request<void>('PATCH', `/api/files/${fileId}/links/${itemId}`, { edition }),
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
  movies: (library?: number) =>
    request<MovieSummary[]>('GET', `/api/movies${library ? `?library=${library}` : ''}`),
  movieDetail: (id: number) => request<MovieDetail>('GET', `/api/movies/${id}`),
  search: (q: string) => request<SearchResults>('GET', `/api/search?q=${encodeURIComponent(q)}`),
  /** Everything about one item, including its number in every ordering. */
  item: (id: number) => request<ItemView>('GET', `/api/items/${id}`),
  itemEvents: (id: number) => request<EventRecord[]>('GET', `/api/items/${id}/events`),
  editItem: (id: number, edit: ItemEdit) => request<void>('PATCH', `/api/items/${id}`, edit),
  setLocks: (id: number, fields: string[]) =>
    request<void>('PUT', `/api/items/${id}/locks`, { fields }),
  refreshItem: (id: number) => request<ImportReport>('POST', `/api/items/${id}/refresh`),
  setMatchOrdering: (seriesId: number, view: string | null) =>
    request<void>('PUT', `/api/series/${seriesId}/match-ordering`, { view }),
  rematchSeries: (seriesId: number, providerId: string) =>
    request<ImportReport>('POST', `/api/series/${seriesId}/rematch`, { provider_id: providerId }),
  rescanSeries: (seriesId: number) => request<void>('POST', `/api/series/${seriesId}/rescan`),
  wanted: (q: WantedQuery = {}) => request<WantedPage>('GET', `/api/wanted?${query(q)}`),
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

  authStatus: () => request<AuthStatus>('GET', '/api/auth/status'),
  setup: (name: string, password: string) =>
    request<User>('POST', '/api/auth/setup', { name, password }),
  login: (name: string, password: string) =>
    request<User>('POST', '/api/auth/login', { name, password }),
  logout: () => request<void>('POST', '/api/auth/logout'),
  updateMe: (changes: {
    name?: string
    password?: string
    current_password?: string
    incognito?: boolean
  }) => request<User>('PATCH', '/api/me', changes),
  users: () => request<User[]>('GET', '/api/users'),
  createUser: (name: string, password: string, isAdmin: boolean) =>
    request<User>('POST', '/api/users', { name, password, is_admin: isAdmin }),
  updateUser: (id: number, changes: { name?: string; password?: string; is_admin?: boolean }) =>
    request<User>('PATCH', `/api/users/${id}`, changes),
  deleteUser: (id: number) => request<void>('DELETE', `/api/users/${id}`),

  versions: (itemId: number) => request<Version[]>('GET', `/api/items/${itemId}/versions`),
  play: (itemId: number, choice: PlayChoice, caps: ClientCaps, startSeconds?: number) =>
    request<PlayPlan>('POST', `/api/items/${itemId}/play`, {
      ...choice,
      caps,
      start_seconds: startSeconds,
    }),
  /** Keeps going when the page is closing, so the last position is saved. */
  stopPlay: (sessionId: string) =>
    request<void>('DELETE', `/api/play/${sessionId}`, undefined, { keepalive: true }),
  progress: (itemId: number, position: number, duration?: number | null) =>
    request<ItemWatch>(
      'PUT',
      `/api/items/${itemId}/progress`,
      { position_seconds: position, duration_seconds: duration ?? undefined },
      { keepalive: true },
    ),
  /** An episode or movie; for a series, all its episodes or just `episodes`. */
  setPlayed: (itemId: number, played: boolean, episodes?: number[]) =>
    request<{ changed: number }>('PUT', `/api/items/${itemId}/played`, { played, episodes }),

  /** What happened, newest first, as far as the signed-in user may see. */
  activity: (q: ActivityQuery = {}) => request<ActivityPage>('GET', `/api/activity?${query(q)}`),
  clearMyWatchActivity: () => request<{ removed: number }>('DELETE', '/api/activity/mine/watch'),

  playlists: (seriesId?: number) =>
    request<Playlist[]>('GET', `/api/playlists${seriesId ? `?series_id=${seriesId}` : ''}`),
  createPlaylist: (input: PlaylistInput) => request<Playlist>('POST', '/api/playlists', input),
  updatePlaylist: (id: number, input: PlaylistInput) =>
    request<Playlist>('PATCH', `/api/playlists/${id}`, input),
  deletePlaylist: (id: number) => request<void>('DELETE', `/api/playlists/${id}`),
}
