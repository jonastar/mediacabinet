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
    }

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

/** One list: folders first (they unblock their files), then files, then unmatched. */
export function entries(q: ReviewQueue): Entry[] {
  return [
    ...q.folders.map((f): Entry => ({
      key: `folder:${f.root_id}:${f.folder_path}`,
      kind: 'folder',
      chip: f.kind === 'movies' ? 'Which movie?' : 'Which series?',
      label: f.folder_path,
      root: f.root_path,
      folder: f,
    })),
    ...q.files.map((f): Entry => ({
      key: `file:${f.file_id}`,
      kind: 'file',
      chip: fileChip(f),
      label: lastPart(f.relative_path),
      root: f.root_path,
      file: f,
    })),
    ...q.unmatched.map((f): Entry => ({
      key: `file:${f.file_id}`,
      kind: 'unmatched',
      chip: f.parent_id ? 'No episode fits' : 'Folder not identified',
      label: lastPart(f.relative_path),
      root: f.root_path,
      file: f,
    })),
  ]
}
