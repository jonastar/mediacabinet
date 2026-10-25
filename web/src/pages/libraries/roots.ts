import type { Root, WritePolicy } from '../../api/client'
import { ago, dayTimeLabel, time } from '../../lib/format'

export const POLICY_NOTES: Record<WritePolicy, string> = {
  managed:
    'Managed: existing files are renamed and moved to match the template. New files are placed by it.',
  adaptive:
    'Adaptive: existing files are never moved. New files go next to their siblings, or into the series’ stored folder using the template.',
  read_only:
    'Read-only: matched but never touched. Nothing is written here; imports and sidecars go elsewhere (sidecars to the cache folder).',
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

/** One line on how the root is doing, and its colour. */
export function rootStatus(r: Root): { text: string; tone: 'ok' | 'busy' | 'bad' | 'idle' } {
  const files = r.files
  if (r.scanning) {
    const s = r.scanning
    const step =
      s.phase === 'probing' && s.total > 0
        ? `Reading streams · ${s.done.toLocaleString()} of ${s.total.toLocaleString()}`
        : 'Scanning files…'
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
