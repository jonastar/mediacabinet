import type { FileReview, FolderReview, ReviewQueue } from '../../api/client'

export type Entry =
  | { key: string; kind: 'folder'; chip: string; label: string; root: string; folder: FolderReview }
  | {
      key: string
      kind: 'file' | 'unmatched'
      chip: string
      label: string
      root: string
      file: FileReview
      /** The series group it's listed under, if any. */
      group?: string
    }
  | { key: string; kind: 'group'; chip: string; label: string; root: string; files: FileReview[] }

/** A short reason for the list, from the top candidate's explanation. */
function fileChip(f: FileReview): string {
  const reason = f.candidates[0]?.reason ?? ''
  if (reason.includes('runtime')) {
    return 'Runtime mismatch'
  }
  if (reason.includes('differs')) {
    return 'Title differs'
  }
  if (reason.includes("isn't in any season")) {
    return 'Number outside season'
  }
  if (reason.includes('season folder')) {
    return 'Season from folder'
  }
  return 'Low confidence'
}

const lastPart = (path: string) => path.split('/').pop() ?? path

function fileEntry(f: FileReview, group?: string): Entry {
  const label = lastPart(f.relative_path)
  if (f.candidates.length > 0) {
    return {
      key: `file:${f.file_id}`,
      kind: 'file',
      chip: fileChip(f),
      label,
      root: f.root_path,
      file: f,
      group,
    }
  }
  const chip = f.parent_id ? 'No episode fits' : 'Folder not identified'
  return {
    key: `file:${f.file_id}`,
    kind: 'unmatched',
    chip,
    label,
    root: f.root_path,
    file: f,
    group,
  }
}

/**
 * One list: folders first (they unblock their files), then files. Files of a
 * series with more than one to review are listed under the series, to decide
 * together; the rest follow, files with candidates before unmatched ones.
 */
export function entries(q: ReviewQueue): Entry[] {
  const folders = q.folders.map((f): Entry => ({
    key: `folder:${f.root_id}:${f.folder_path}`,
    kind: 'folder',
    chip: f.kind === 'movies' ? 'Which movie?' : 'Which series?',
    label: f.folder_path,
    root: f.root_path,
    folder: f,
  }))
  const all = [...q.files, ...q.unmatched]
  const bySeries = new Map<number, FileReview[]>()
  for (const f of all) {
    if (f.parent_id != null && f.kind === 'tv') {
      bySeries.set(f.parent_id, [...(bySeries.get(f.parent_id) ?? []), f])
    }
  }
  const groups = [...bySeries.entries()]
    .filter(([, files]) => files.length > 1)
    .sort(([, a], [, b]) => (a[0].parent_title ?? '').localeCompare(b[0].parent_title ?? ''))
  const grouped = new Set(groups.flatMap(([, files]) => files.map((f) => f.file_id)))

  return [
    ...folders,
    ...groups.flatMap(([id, files]) => {
      const key = `group:${id}`
      const header: Entry = {
        key,
        kind: 'group',
        chip: `${files.length} files`,
        label: files[0].parent_title ?? 'Series',
        root: files[0].root_path,
        files,
      }
      return [header, ...files.map((f) => fileEntry(f, key))]
    }),
    ...all.filter((f) => !grouped.has(f.file_id)).map((f) => fileEntry(f)),
  ]
}
