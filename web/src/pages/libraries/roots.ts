import type { Library, RefreshProgress, Root, ScanProgress, WritePolicy } from '../../api/client'
import { ago, dayTimeLabel, time } from '../../lib/format'

export const POLICY_NOTES: Record<WritePolicy, string> = {
  managed: 'Existing files are renamed and moved to match the layout. New files are placed by it.',
  adaptive:
    'Existing files are never moved. New files go next to their siblings, or into the series’ stored folder using the layout.',
  read_only:
    'Matched but never touched. Nothing is written here; imports and sidecars go elsewhere (sidecars to the cache folder).',
}

export const POLICIES: [WritePolicy, string][] = [
  ['managed', 'Managed'],
  ['adaptive', 'Adaptive'],
  ['read_only', 'Read-only'],
]

const count = (n: number, noun: string) => `${n.toLocaleString()} ${noun}${n === 1 ? '' : 's'}`

function since(seconds: number) {
  return Date.now() / 1000 - seconds < 86400 ? time(seconds) : dayTimeLabel(seconds)
}

const ofTotal = (s: ScanProgress | RefreshProgress) =>
  `${s.done.toLocaleString()} of ${s.total.toLocaleString()}`

function scanStep(s: ScanProgress) {
  switch (s.phase) {
    case 'walking':
      return s.done > 0
        ? `Looking for files · ${count(s.done, 'video')} found`
        : 'Looking for files…'
    case 'hashing':
      return s.total > 0 ? `Fingerprinting new files · ${ofTotal(s)}` : 'Fingerprinting new files…'
    case 'probing':
      return s.total > 0 ? `Reading streams · ${ofTotal(s)}` : 'Reading streams…'
    case 'matching':
      return s.total > 0 ? `Matching folders · ${ofTotal(s)}` : 'Matching folders…'
  }
}

/** One line on how the root is doing, and its colour. */
export function rootStatus(r: Root): { text: string; tone: 'ok' | 'busy' | 'bad' | 'idle' } {
  const files = r.files
  if (r.scanning) {
    const s = r.scanning
    const step = scanStep(s)
    return { text: s.queued_again ? `${step} · another scan queued` : step, tone: 'busy' }
  }
  if (!r.online) {
    const when = r.offline_since ? ` since ${since(r.offline_since)}` : ''
    const known = files.present + files.missing
    return { text: `Offline${when} · ${count(known, 'file')} kept, nothing removed`, tone: 'bad' }
  }
  const last = r.last_scan
  if (last?.state === 'failed') {
    return { text: `Last scan failed · ${last.error ?? 'unknown error'}`, tone: 'bad' }
  }
  if (!last) {
    return {
      text: files.present
        ? `Online · ${count(files.present, 'file')} · never scanned`
        : 'Online · not scanned yet',
      tone: 'idle',
    }
  }
  const missing = files.missing ? ` · ${count(files.missing, 'file')} missing` : ''
  const scanned = last.finished_at ? ` · scanned ${ago(last.finished_at)}` : ''
  return { text: `Online · ${count(files.present, 'file')}${missing}${scanned}`, tone: 'ok' }
}

/** One line on the library's last or running metadata refresh, and its colour. */
export function refreshStatus(library: Library): {
  text: string
  tone: 'ok' | 'busy' | 'bad' | 'idle'
} {
  const r = library.refreshing
  if (r) {
    const step = r.total > 0 ? `Refreshing · ${ofTotal(r)}` : 'Refreshing…'
    return { text: r.queued_again ? `${step} · another refresh queued` : step, tone: 'busy' }
  }
  const last = library.last_refresh
  if (!last) {
    return { text: 'Refreshed on the schedule only', tone: 'idle' }
  }
  if (last.state === 'failed') {
    return { text: `Last refresh failed · ${last.error ?? 'unknown error'}`, tone: 'bad' }
  }
  const when = last.finished_at ? ` ${ago(last.finished_at)}` : ''
  if (last.error) {
    return { text: `Refreshed${when} · ${last.error}`, tone: 'bad' }
  }
  const n = Number((last.report as { refreshed?: number } | null)?.refreshed ?? 0)
  return { text: `Refreshed ${count(n, 'title')}${when}`, tone: 'ok' }
}

/** What a library holds, in a line. */
export function librarySummary(library: Library) {
  const files = library.roots.reduce((n, r) => n + r.files.present, 0)
  const kind = library.kind === 'movies' ? 'Movies' : 'TV shows'
  return `${kind} · ${count(library.roots.length, 'folder')} · ${count(files, 'file')}`
}
