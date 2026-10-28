import { useState } from 'react'
import {
  api,
  type Candidate,
  type FileReview,
  type LibraryKind,
  type PathPiece,
} from '../../api/client'
import { barColor, pct } from './confidence'
import type { Entry } from './entries'
import { EpisodePicker, SearchPicker } from './Pickers'
import styles from './Review.module.css'

interface Fact {
  k: string
  v: string
  src: string
}

function PathLine({ pieces }: { pieces: PathPiece[] }) {
  return (
    <p className={styles.path}>
      {pieces.map((p, i) => (
        <span key={i}>
          {i > 0 && <span className={styles.slash}>/</span>}
          <span className={`${styles.seg} ${styles[p.role] ?? ''}`}>{p.text}</span>
        </span>
      ))}
    </p>
  )
}

function Facts({ facts }: { facts: Fact[] }) {
  return (
    <div className={styles.facts}>
      {facts.map((f) => (
        <div key={f.k} className={styles.fact}>
          <span className={styles.factKey}>{f.k}</span>
          <span className={styles.factValue}>{f.v}</span>
          <span className={styles.factSource}>{f.src}</span>
        </div>
      ))}
    </div>
  )
}

const SOURCES: Record<string, string> = {
  parser: 'parser',
  path_id: 'ID in path',
  nfo: '.nfo',
  search: 'title search',
  ai: 'ai tie-break',
}

function Candidates({
  candidates,
  picked,
  onPick,
}: {
  candidates: Candidate[]
  picked: number | null
  onPick: (id: number) => void
}) {
  return (
    <fieldset className={styles.candidates}>
      <legend className={styles.label}>Candidates</legend>
      {candidates.map((c) => (
        <label key={c.id} className={styles.candidate}>
          <input
            type="radio"
            name="candidate"
            checked={picked === c.id}
            onChange={() => onPick(c.id)}
          />
          <span className={styles.candidateText}>
            <span className={styles.candidateTitle}>{c.title}</span>
            <span className={styles.candidateReason}>{c.reason}</span>
          </span>
          <span className={styles.confidence}>
            <span className={styles.confidenceLine}>
              <span>{SOURCES[c.source] ?? c.source}</span>
              <b>{pct(c.confidence)}</b>
            </span>
            <span className={styles.bar} aria-hidden="true">
              <span style={{ width: pct(c.confidence), background: barColor(c.confidence) }} />
            </span>
          </span>
        </label>
      ))}
    </fieldset>
  )
}

function fileFacts(f: FileReview): Fact[] {
  const p = f.parsed
  const facts: Fact[] = [
    {
      k: 'Series',
      v: f.parent_title ?? 'not identified',
      src: f.parent_title ? 'from the series folder' : 'folder needs review',
    },
  ]
  if (p.season != null && p.episodes.length > 0) {
    const eps = p.episodes.map((e) => `E${String(e).padStart(2, '0')}`).join(' ')
    facts.push({
      k: 'Season / ep',
      v: `S${String(p.season).padStart(2, '0')} ${eps}`,
      src: 'filename parser',
    })
  }
  if (p.absolute.length > 0) {
    facts.push({ k: 'Number', v: p.absolute.join(', '), src: 'filename parser' })
  }
  if (p.episode_title) {
    facts.push({ k: 'Episode title', v: p.episode_title, src: 'filename parser' })
  }
  if (f.duration_seconds) {
    const d = f.duration_seconds
    const runtime = d < 90 ? `${Math.round(d)} s` : `${Math.round(d / 60)} min`
    facts.push({ k: 'Runtime', v: runtime, src: 'ffprobe' })
  }
  if (p.edition) {
    facts.push({ k: 'Edition', v: p.edition, src: 'filename parser' })
  }
  if (p.resolution) {
    facts.push({ k: 'Resolution', v: p.resolution, src: 'filename parser' })
  }
  if (p.release_group) {
    facts.push({ k: 'Release group', v: p.release_group, src: 'filename parser' })
  }
  return facts
}

export function Detail({
  entry,
  onDone,
}: {
  entry: Exclude<Entry, { kind: 'group' }>
  onDone: () => void
}) {
  const candidates = entry.kind === 'folder' ? entry.folder.candidates : entry.file.candidates
  const [picked, setPicked] = useState<number | null>(candidates[0]?.id ?? null)
  const [mode, setMode] = useState<'candidates' | 'search'>(
    candidates.length ? 'candidates' : 'search',
  )
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const act = async (action: () => Promise<unknown>) => {
    setBusy(true)
    setError(null)
    try {
      await action()
      onDone()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  let facts: Fact[]
  let why: string
  let context: { root: string; path: PathPiece[] }
  let picker
  if (entry.kind === 'folder') {
    const f = entry.folder
    const noun = f.kind === 'movies' ? 'movie' : 'series'
    facts = [
      { k: 'Title', v: f.hints.title || '—', src: `from the ${noun} folder` },
      {
        k: 'Year hint',
        v: f.hints.year ? String(f.hints.year) : 'none',
        src: f.hints.year ? 'folder name' : '',
      },
      { k: 'Files', v: String(f.files), src: 'waiting on this folder' },
    ]
    if (f.hints.ids.length > 0) {
      facts.push({
        k: 'IDs',
        v: f.hints.ids.map(([p, v]) => `${p}:${v}`).join(', '),
        src: 'folder name',
      })
    }
    why = `No confident match for this folder, so nothing was imported. Choose the ${noun}, and its files are matched straight away. Grouping folders (blue) are only hints.`
    context = { root: f.root_path, path: f.path_pieces }
    picker = (
      <SearchPicker
        kind={f.kind as LibraryKind}
        initial={f.hints.title}
        year={f.hints.year}
        onPick={(r) => act(() => api.identifyFolder(f.root_id, f.folder_path, r.id))}
      />
    )
  } else {
    const f = entry.file
    facts = fileFacts(f)
    why =
      entry.kind === 'file'
        ? `${f.candidates[0]?.reason ?? 'Below the confidence threshold'}. Whatever you accept is pinned and survives every rescan.`
        : f.parent_id
          ? `No episode of ${f.parent_title} has the numbers in this name. Choose the episode by hand, or ignore the file.`
          : f.item_folder
            ? `Its folder “${f.item_folder}” isn’t identified: the search found nothing confident. Search for the ${f.kind === 'movies' ? 'movie' : 'series'} below.`
            : 'It isn’t inside a series folder of this root’s template, so it can’t be matched. Move it into one, or ignore it.'
    context = { root: f.root_path, path: f.path_pieces }
    const folder = f.item_folder
    picker = f.parent_id ? (
      <EpisodePicker
        seriesId={f.parent_id}
        onPick={(ids) => act(() => api.linkFile(f.file_id, ids, true))}
      />
    ) : folder ? (
      <SearchPicker
        kind={f.kind as LibraryKind}
        initial={f.folder_hints?.title || f.parsed.title || ''}
        year={f.folder_hints?.year ?? f.parsed.year}
        onPick={(r) => act(() => api.identifyFolder(f.root_id, folder, r.id))}
      />
    ) : null
  }

  const accept = () =>
    picked !== null && act(() => api.acceptCandidate(picked, entry.kind !== 'folder'))

  return (
    <section aria-labelledby="review-detail" className={styles.detail}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
        <h2 id="review-detail" className={styles.label}>
          {entry.kind === 'folder' ? 'Folder' : 'File'}
        </h2>
        <PathLine pieces={context.path} />
        <p className={styles.context}>
          Root <code>{context.root}</code>
        </p>
      </div>
      <Facts facts={facts} />
      <div className={styles.why}>{why}</div>
      {mode === 'candidates' && candidates.length > 0 ? (
        <Candidates candidates={candidates} picked={picked} onPick={setPicked} />
      ) : (
        picker
      )}
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        {mode === 'candidates' && candidates.length > 0 && (
          <button
            type="button"
            className={`${styles.button} ${styles.primary}`}
            disabled={busy || picked === null}
            onClick={accept}
          >
            {entry.kind === 'folder' ? 'Accept' : 'Accept and pin'}
          </button>
        )}
        {candidates.length > 0 && (
          <button
            type="button"
            className={styles.button}
            disabled={busy}
            onClick={() => setMode(mode === 'candidates' ? 'search' : 'candidates')}
          >
            {mode === 'candidates' ? 'Search for another match' : 'Back to candidates'}
          </button>
        )}
        {entry.kind !== 'folder' && (
          <button
            type="button"
            className={styles.button}
            disabled={busy}
            onClick={() => act(() => api.ignoreFile(entry.file.file_id))}
          >
            Ignore this file
          </button>
        )}
      </div>
    </section>
  )
}
