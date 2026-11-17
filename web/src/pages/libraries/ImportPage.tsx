import { useState } from 'react'
import {
  api,
  type ImportCandidate,
  type ImportListing,
  type Library,
  type Naming,
  type TransferMode,
} from '../../api/client'
import { useApi } from '../../api/useApi'
import { BackIcon } from '../../components/Icons'
import { Link } from '../../components/Link'
import { Failed, Loading } from '../../components/Status'
import { LOCALE } from '../../core/locale'
import { episodeLabel } from '../../lib/format'
import { useQueryParam } from '../../lib/router'
import { NotFoundPage } from '../NotFoundPage'
import { FolderBrowser } from './FolderBrowser'
import { useLibraries } from './useLibraries'
import styles from './Libraries.module.css'
import own from './OrganizePage.module.css'

const count = (n: number, noun: string) =>
  `${n.toLocaleString(LOCALE)} ${noun}${n === 1 ? '' : 's'}`

function size(bytes: number) {
  const gb = bytes / 1024 ** 3
  return gb >= 1
    ? `${gb.toLocaleString(LOCALE, { maximumFractionDigits: 1 })} GB`
    : `${Math.max(1, Math.round(bytes / 1024 ** 2)).toLocaleString(LOCALE)} MB`
}

/** What a file is to be imported as: episodes of one series, or a movie. */
interface Match {
  items: number[]
  label: string
}

/** One file's state on the page. */
interface Row {
  file: ImportCandidate
  chosen: boolean
  match: Match | null
  /** Where a dry run says it goes, relative to the library folder. */
  destination?: string
  note?: string
  done?: boolean
  error?: string
}

function guessed(file: ImportCandidate): Match | null {
  const g = file.guess
  if (!g) {
    return null
  }
  const name = g.year ? `${g.title} (${g.year})` : g.title
  if (g.kind === 'movie') {
    return { items: [g.item_id], label: name }
  }
  if (g.episodes.length === 0) {
    return null
  }
  const eps = g.episodes.map((e) => e.label).join(' + ')
  const title = g.episodes.length === 1 ? ` · ${g.episodes[0].title}` : ''
  return { items: g.episodes.map((e) => e.id), label: `${name} · ${eps}${title}` }
}

const MODES: { mode: TransferMode; label: string; note: string }[] = [
  {
    mode: 'copy',
    label: 'Copy',
    note: 'The original stays where it is, untouched. Needs space for a second copy.',
  },
  {
    mode: 'hardlink',
    label: 'Hard link',
    note: 'No extra space, and a download client keeps seeding. Only on the same disk; elsewhere it copies.',
  },
  {
    mode: 'move',
    label: 'Move',
    note: 'The original is moved into the library. A download client loses it.',
  },
]

const NAMINGS: { naming: Naming; label: string; note: string }[] = [
  {
    naming: 'layout',
    label: 'Rename to the layout',
    note: 'Named and placed like the folder’s layout says.',
  },
  {
    naming: 'keep',
    label: 'Keep original names',
    note: 'Next to the season’s other files if there are any, otherwise in a folder made by the layout.',
  },
]

/** Picking by hand what a file is: a movie, or a series and one of its episodes. */
function MatchPicker({
  library,
  onPick,
  onCancel,
}: {
  library: Library
  onPick: (match: Match) => void
  onCancel: () => void
}) {
  const isMovies = library.kind === 'movies'
  const titles = useApi(`import-titles:${library.id}`, async () =>
    isMovies
      ? (await api.movies(library.id)).map((m) => ({ id: m.id, title: m.title, year: m.year }))
      : (await api.series(library.id)).map((s) => ({ id: s.id, title: s.title, year: s.year })),
  )
  const [item, setItem] = useState<number | null>(null)
  const episodes = useApi(isMovies || item == null ? null : `import-episodes:${item}`, () =>
    api.episodes(item ?? 0),
  )
  const chosen = titles.data?.find((t) => t.id === item)
  const name = chosen ? (chosen.year ? `${chosen.title} (${chosen.year})` : chosen.title) : ''

  return (
    <div className={own.picker}>
      <select
        className={styles.input}
        aria-label={isMovies ? 'Movie' : 'Series'}
        value={item ?? ''}
        onChange={(e) => {
          const id = Number(e.target.value)
          setItem(id || null)
          const t = titles.data?.find((x) => x.id === id)
          if (isMovies && t) {
            onPick({ items: [id], label: t.year ? `${t.title} (${t.year})` : t.title })
          }
        }}
      >
        <option value="">
          {titles.data ? `Choose a ${isMovies ? 'movie' : 'series'}…` : 'Loading…'}
        </option>
        {titles.data?.map((t) => (
          <option key={t.id} value={t.id}>
            {t.year ? `${t.title} (${t.year})` : t.title}
          </option>
        ))}
      </select>
      {!isMovies && item != null && (
        <select
          className={styles.input}
          aria-label="Episode"
          value=""
          onChange={(e) => {
            const ep = episodes.data?.entries.find((x) => x.item_id === Number(e.target.value))
            if (ep) {
              const num = ep.number ? `${episodeLabel(ep.number)} · ` : ''
              onPick({ items: [ep.item_id], label: `${name} · ${num}${ep.title}` })
            }
          }}
        >
          <option value="">{episodes.data ? 'Choose an episode…' : 'Loading…'}</option>
          {episodes.data?.entries
            .filter((ep) => !ep.is_gap)
            .map((ep) => (
              <option key={ep.item_id} value={ep.item_id}>
                {ep.number ? `${episodeLabel(ep.number)} · ` : ''}
                {ep.title}
              </option>
            ))}
        </select>
      )}
      <button type="button" className={`${styles.button} ${styles.small}`} onClick={onCancel}>
        Cancel
      </button>
    </div>
  )
}

function FileRow({
  row,
  library,
  disabled,
  onChange,
}: {
  row: Row
  library: Library
  disabled: boolean
  onChange: (change: Partial<Row>) => void
}) {
  const [picking, setPicking] = useState(false)
  const f = row.file
  return (
    <li className={own.group}>
      <label className={own.move} style={{ background: 'none', padding: 0 }}>
        <input
          type="checkbox"
          checked={row.chosen}
          disabled={disabled || !row.match || row.done}
          onChange={(e) => onChange({ chosen: e.target.checked })}
        />
        <span className={own.paths}>
          <span className={own.to}>{f.relative_path}</span>
          <span className={own.from}>
            {size(f.size)}
            {!f.same_disk && ' · on another disk than the library'}
          </span>
        </span>
      </label>
      {f.waiting && <span className={own.skipped}>Not finished yet? {f.waiting}.</span>}
      {picking ? (
        <MatchPicker
          library={library}
          onCancel={() => setPicking(false)}
          onPick={(match) => {
            setPicking(false)
            onChange({ match, chosen: true, destination: undefined, error: undefined })
          }}
        />
      ) : (
        <div className={styles.row}>
          <span className={row.match ? undefined : styles.muted}>
            {row.match?.label ?? 'Not recognised: choose what it is.'}
          </span>
          {!row.done && (
            <button
              type="button"
              className={styles.link}
              disabled={disabled}
              onClick={() => setPicking(true)}
            >
              {row.match ? 'Change' : 'Choose…'}
            </button>
          )}
        </div>
      )}
      {row.destination && (
        <span className={own.paths}>
          <span className={own.to}>
            <span aria-hidden="true">{row.done ? '✓ ' : '→ '}</span>
            {row.destination}
          </span>
          {row.note && <span className={own.from}>{row.note}</span>}
        </span>
      )}
      {row.error && <span className={styles.error}>{row.error}</span>}
    </li>
  )
}

function Files({ library, listing }: { library: Library; listing: ImportListing }) {
  const [rows, setRows] = useState<Row[]>(() =>
    listing.files.map((file) => {
      const match = guessed(file)
      return { file, match, chosen: match != null && !file.waiting }
    }),
  )
  const [mode, setMode] = useState<TransferMode>('copy')
  const [naming, setNaming] = useState<Naming>('layout')
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null)
  const [previewed, setPreviewed] = useState(false)
  const busy = progress != null
  const chosen = rows.filter((r) => r.chosen && r.match && !r.done)
  const anySameDisk = listing.files.some((f) => f.same_disk)

  const update = (i: number, change: Partial<Row>) => {
    setRows((current) => current.map((r, j) => (j === i ? { ...r, ...change } : r)))
    if ('match' in change) {
      setPreviewed(false)
    }
  }
  const reset = () => {
    setPreviewed(false)
    setRows((current) =>
      current.map((r) => (r.done ? r : { ...r, destination: undefined, error: undefined })),
    )
  }

  /** Each chosen file in turn: a dry run to preview, or the import itself. */
  const run = async (dryRun: boolean) => {
    const todo = rows.map((r, i) => ({ r, i })).filter(({ r }) => r.chosen && r.match && !r.done)
    setProgress({ done: 0, total: todo.length })
    let failed = false
    for (const [n, { r, i }] of todo.entries()) {
      try {
        const result = await api.importFile({
          path: r.file.path,
          items: r.match?.items ?? [],
          mode,
          naming,
          dry_run: dryRun,
        })
        update(i, {
          destination: result.path,
          note: result.note,
          error: undefined,
          ...(dryRun ? {} : { done: true, chosen: false }),
        })
      } catch (e) {
        failed = true
        update(i, { error: (e as Error).message, destination: undefined })
      }
      setProgress({ done: n + 1, total: todo.length })
    }
    setProgress(null)
    setPreviewed(dryRun && !failed)
  }

  const imported = rows.filter((r) => r.done).length
  const unknown = rows.filter((r) => !r.match).length
  return (
    <>
      <p className={styles.muted} role="status">
        {[
          count(listing.files.length, 'video'),
          unknown ? `${unknown.toLocaleString(LOCALE)} not recognised` : null,
          listing.unfinished ? `${count(listing.unfinished, 'unfinished file')} skipped` : null,
          imported ? `${imported.toLocaleString(LOCALE)} imported` : null,
        ]
          .filter(Boolean)
          .join(' · ')}
        {listing.truncated && ' · only the first 1,000 are listed'}
      </p>

      {listing.files.length > 0 && (
        <>
          <div className={styles.block}>
            <span className={styles.label}>How</span>
            <div role="radiogroup" aria-label="How" className={styles.policyOptions}>
              {MODES.map((m) => (
                <label key={m.mode} className={styles.policyOption}>
                  <input
                    type="radio"
                    name="import-mode"
                    checked={mode === m.mode}
                    disabled={busy || (m.mode === 'hardlink' && !anySameDisk)}
                    onChange={() => {
                      setMode(m.mode)
                      reset()
                    }}
                  />
                  <span className={styles.policyText}>
                    <b>{m.label}</b>
                    <span>
                      {m.mode === 'hardlink' && !anySameDisk
                        ? 'Not possible here: these files are on another disk than the library.'
                        : m.note}
                    </span>
                  </span>
                </label>
              ))}
            </div>
          </div>
          <div className={styles.block}>
            <span className={styles.label}>Names</span>
            <div role="radiogroup" aria-label="Names" className={styles.policyOptions}>
              {NAMINGS.map((n) => (
                <label key={n.naming} className={styles.policyOption}>
                  <input
                    type="radio"
                    name="import-naming"
                    checked={naming === n.naming}
                    disabled={busy}
                    onChange={() => {
                      setNaming(n.naming)
                      reset()
                    }}
                  />
                  <span className={styles.policyText}>
                    <b>{n.label}</b>
                    <span>{n.note}</span>
                  </span>
                </label>
              ))}
            </div>
          </div>

          <div className={own.bar}>
            {previewed ? (
              <button
                type="button"
                className={`${styles.button} ${styles.primary}`}
                disabled={busy || chosen.length === 0}
                onClick={() => run(false)}
              >
                {progress
                  ? `Importing… ${progress.done} of ${progress.total}`
                  : `Import ${count(chosen.length, 'file')}`}
              </button>
            ) : (
              <button
                type="button"
                className={`${styles.button} ${styles.primary}`}
                disabled={busy || chosen.length === 0}
                onClick={() => run(true)}
              >
                {progress
                  ? `Checking… ${progress.done} of ${progress.total}`
                  : `Preview ${count(chosen.length, 'file')}`}
              </button>
            )}
            <span className={styles.muted}>
              {previewed
                ? 'Each file goes where it shows. Sidecars come along. Nothing is overwritten.'
                : 'See where each file goes before anything is written.'}
            </span>
          </div>

          <ul className={own.groups}>
            {rows.map((row, i) => (
              <FileRow
                key={row.file.path}
                row={row}
                library={library}
                disabled={busy}
                onChange={(change) => update(i, change)}
              />
            ))}
          </ul>
        </>
      )}
    </>
  )
}

/** The import tool: pick a folder, check what each video is, and bring them into the library. */
export function ImportPage({ libraryId }: { libraryId: number }) {
  const libraries = useLibraries()
  const given = useQueryParam('path')
  const [path, setPath] = useState(given ?? '')
  // The folder looked at, and how many times: each look is listed afresh. A
  // folder given in the link, such as the inbox, is looked at straight away.
  const [looking, setLooking] = useState(given ? { at: given, n: 0 } : null)
  const listing = useApi(looking && `imports:${libraryId}:${looking.n}:${looking.at}`, () =>
    api.listImports(libraryId, looking?.at ?? ''),
  )
  const look = (at: string) => setLooking((l) => ({ at, n: (l?.n ?? 0) + 1 }))
  const busy = listing.loading

  if (libraries.error) {
    return <Failed error={libraries.error} onRetry={libraries.reload} />
  }
  if (!libraries.data) {
    return <Loading />
  }
  const library = libraries.data.find((l) => l.id === libraryId)
  if (!library || library.kind === 'folders') {
    return <NotFoundPage />
  }
  const things = library.kind === 'movies' ? 'movies' : 'episodes'
  const target = library.roots.find((r) => r.is_import_target)

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <Link to={`/manage/libraries/${library.id}`} className={styles.back}>
          <BackIcon />
          {library.name}
        </Link>
        <div className={styles.heading}>
          <h1>Import</h1>
          <p className={styles.lead}>
            Bring {things} from a folder on the server into {library.name}. Each video is matched to
            what’s in the library; check it, then see where it goes before anything is written.
          </p>
        </div>
        {!target && (
          <div className={`${styles.panel} ${styles.warning}`} role="alert">
            <strong>{library.name} has no folder for new items.</strong>
            <p className={styles.muted}>
              Files for {library.kind === 'movies' ? 'movies' : 'series'} already in a writable
              folder can still be imported. Choose a folder for new items on{' '}
              <Link to={`/manage/libraries/${library.id}`}>the library’s page</Link>.
            </p>
          </div>
        )}
        <section aria-label="Folder" className={styles.panel}>
          <FolderBrowser
            id="import-path"
            label="Folder on the server"
            value={path}
            placeholder="/downloads/complete"
            action={busy ? 'Looking…' : 'Look at folder'}
            busy={busy}
            onChange={setPath}
            onSubmit={look}
          />
          {listing.error && (
            <p className={styles.error} role="alert">
              {listing.error.message}
            </p>
          )}
          <p className={styles.muted}>
            Only what’s already in {library.name} is recognised: add a new series or movie first,
            then import its files.
          </p>
        </section>
        {listing.data && !listing.loading && !listing.error && (
          <Files key={looking?.n} library={library} listing={listing.data} />
        )}
      </div>
    </div>
  )
}
