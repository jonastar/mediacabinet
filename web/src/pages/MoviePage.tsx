import { useRef, useState } from 'react'
import { api, artworkUrl, type LinkedFile, type Version } from '../api/client'
import { useApi } from '../api/useApi'
import { EditDetails } from '../components/EditDetails'
import { ChevronDownIcon, PencilIcon, PlayIcon, PlusIcon } from '../components/Icons'
import { LibraryBack } from '../components/LibraryBack'
import { Link } from '../components/Link'
import { CastSection } from '../components/People'
import { Streams } from '../components/Streams'
import { Failed, Loading } from '../components/Status'
import { audioLabel, codecName } from '../core/labels'
import {
  ago,
  airDate,
  bytes,
  hdrName,
  matchSummary,
  resolution,
  runtime,
  seriesTone,
} from '../lib/format'
import { crewNames } from '../lib/people'
import { useMode } from '../lib/prefs'
import { itemPath, useCanonicalPath } from '../lib/router'
import { DetailArtwork, DetailHeader, MetaLine } from './detail/DetailHeader'
import detail from './detail/Detail.module.css'
import { LOCKABLE, facts, lockedLabel, matchedTo } from './detail/metadata'
import styles from './MoviePage.module.css'

const SHORT_HDR: Record<string, string> = { dolby_vision: 'DV', hdr10: 'HDR10', hlg: 'HLG' }

/** `Extended · 2160p`. */
function versionName(f: LinkedFile) {
  return (
    [f.edition, resolution(f.height), f.part ? `part ${f.part}` : null]
      .filter(Boolean)
      .join(' · ') || 'Main version'
  )
}

/** `4K DV`, for the Play button. */
function shortName(f: LinkedFile) {
  const height = resolution(f.height)
  return (
    [height === '2160p' ? '4K' : height, f.hdr_format ? SHORT_HDR[f.hdr_format] : null]
      .filter(Boolean)
      .join(' ') || 'Version'
  )
}

function chips(f: LinkedFile, version: Version | undefined) {
  const audio = version?.audio[0]
  return [
    f.hdr_format ? hdrName(f.hdr_format) : 'SDR',
    codecName(f.video_codec),
    audio ? audioLabel(audio, 0) : null,
  ].filter((c): c is string => Boolean(c))
}

/** A movie, or with `folderVideo` a video of a folder library, which has no metadata. */
export function MoviePage({ id, folderVideo = false }: { id: number; folderVideo?: boolean }) {
  const [mode] = useMode()
  const movie = useApi(`movie:${id}`, () => api.movieDetail(id))
  const about = useApi(`about:${id}`, () => api.about(id))
  const versions = useApi(`versions:${id}`, () => api.versions(id))
  const libraries = useApi('libraries', api.libraries)
  useCanonicalPath(movie.data ? itemPath('movie', id, movie.data.library_id) : null)
  const [editing, setEditing] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  /** The version picked here; until then the server's first choice plays. */
  const [picked, setPicked] = useState<number | null>(null)
  const [menuOpen, setMenuOpen] = useState(false)
  const menu = useRef<HTMLDivElement>(null)
  if (movie.error) {
    return <Failed error={movie.error} onRetry={movie.reload} />
  }
  if (!movie.data) {
    return <Loading />
  }
  const m = movie.data
  const manage = mode === 'manage'
  const tone = seriesTone(m.title)
  const poster = artworkUrl(m, 'poster')
  const frame = artworkUrl(m, 'thumbnail')
  const present = m.files.filter((f) => f.status === 'present')
  const folder = m.files[0]?.relative_path.split('/').slice(0, -1).join('/') ?? ''
  const locked = m.locked_fields.length > 0
  const played = m.watch?.played ?? false
  const resuming = (m.watch?.position_seconds ?? 0) > 0 && !played

  const serverChoice = versions.data?.[0]?.file_id ?? present[0]?.file_id
  const selectedId = present.some((f) => f.file_id === picked) ? picked : serverChoice
  const selected = present.find((f) => f.file_id === selectedId)
  const multiple = present.length > 1
  const playLink =
    picked != null && selectedId === picked ? `/play/${m.id}?file=${picked}` : `/play/${m.id}`
  // The order the server ranks them in, then what isn't playable now.
  const rank = (f: LinkedFile) => {
    const i = versions.data?.findIndex((v) => v.file_id === f.file_id) ?? -1
    return i < 0 ? Number.MAX_SAFE_INTEGER : i
  }
  const files = [...m.files].sort((a, b) => rank(a) - rank(b))

  const run = async (label: string, action: () => Promise<unknown>) => {
    setNotice(`${label}…`)
    try {
      await action()
      setNotice(`${label} done.`)
      await Promise.all([movie.reload(), about.reload()])
    } catch (e) {
      setNotice(`${label} failed: ${(e as Error).message}`)
    }
  }

  const viewerFacts = folderVideo
    ? facts(
        { label: 'File', value: present[0] && resolution(present[0].height) },
        { label: 'Size', value: present[0] && bytes(present[0].size) },
      )
    : facts(
        { label: 'Director', value: crewNames(about.data, ['Director']) },
        { label: 'Writer', value: crewNames(about.data, ['Screenplay', 'Writer']) },
        { label: 'Studio', value: about.data?.studios[0] },
        { label: 'Released', value: m.release != null ? airDate(m.release, false) : m.year },
      )
  const location = m.files[0]
  const editorFacts = facts(
    { label: 'Matched to', value: !folderVideo && matchedTo(m.external_ids) },
    { label: 'Library', value: libraries.data?.find((l) => l.id === m.library_id)?.name },
    {
      label: 'Path',
      value: location && [location.root_path.replace(/\/$/, ''), folder].filter(Boolean).join('/'),
    },
    { label: 'Locked fields', value: !folderVideo && lockedLabel(m.locked_fields) },
  )

  return (
    <DetailHeader
      titleId="movie-title"
      backdrop={artworkUrl(m, 'backdrop') ?? frame}
      tone={tone}
      wide={!poster && Boolean(frame)}
      back={
        <LibraryBack
          libraryId={m.library_id}
          fallback={{ to: '/movies', label: 'Movies' }}
          folder={folderVideo ? folder : undefined}
          className={detail.back}
        />
      }
      barActions={
        manage &&
        !folderVideo && (
          <>
            <button
              type="button"
              className={detail.barButton}
              onClick={() => run('Refresh', () => api.refreshItem(id))}
            >
              Refresh metadata
            </button>
            <button
              type="button"
              className={detail.barButton}
              disabled
              title="Changing a movie's match isn't built yet"
            >
              Fix match
            </button>
          </>
        )
      }
      artwork={
        <DetailArtwork
          src={poster ?? frame}
          tone={tone}
          wide={!poster && Boolean(frame)}
          label={m.title}
          editing={manage}
        >
          {manage && (
            <button
              type="button"
              className={detail.changeArt}
              disabled
              title="Changing artwork isn't built yet"
            >
              Change artwork
            </button>
          )}
        </DetailArtwork>
      }
      facts={manage ? editorFacts : viewerFacts}
      below={
        <>
          {manage && editing && (
            <EditDetails
              itemId={id}
              title={m.title}
              year={m.year}
              overview={m.overview}
              onSaved={() => {
                setEditing(false)
                movie.reload()
              }}
              onCancel={() => setEditing(false)}
            />
          )}
          {!manage && !folderVideo && <CastSection about={about.data} />}
        </>
      }
    >
      <div className={detail.titleRow}>
        <h1 id="movie-title" className={detail.title}>
          {m.title}
        </h1>
        {manage && (
          <button
            type="button"
            className={`${detail.round} ${detail.small}`}
            aria-label="Edit title"
            onClick={() => setEditing(true)}
          >
            <PencilIcon />
          </button>
        )}
      </div>
      <MetaLine parts={[m.year, m.genres[0], runtime(m.runtime_seconds)]} about={about.data} />
      {about.data?.tagline && <p className={detail.tagline}>{about.data.tagline}</p>}
      {m.overview && <p className={detail.overview}>{m.overview}</p>}

      {manage ? (
        <div className={detail.actions}>
          <button type="button" className={detail.primary} onClick={() => setEditing(!editing)}>
            Edit metadata
          </button>
          {!folderVideo && (
            <button
              type="button"
              className={detail.secondary}
              onClick={() =>
                run(locked ? 'Unlock' : 'Lock', () => api.setLocks(id, locked ? [] : LOCKABLE))
              }
            >
              {locked ? 'Unlock fields' : 'Lock fields'}
            </button>
          )}
        </div>
      ) : (
        <div className={detail.actions}>
          {present.length > 0 ? (
            <>
              <div
                ref={menu}
                className={styles.split}
                onBlur={(e) => {
                  if (!menu.current?.contains(e.relatedTarget)) {
                    setMenuOpen(false)
                  }
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Escape') {
                    setMenuOpen(false)
                  }
                }}
              >
                <Link to={playLink} className={styles.splitPlay}>
                  <PlayIcon />
                  {resuming ? 'Resume' : 'Play'}
                </Link>
                {multiple && selected && (
                  <button
                    type="button"
                    className={styles.splitCaret}
                    aria-label={`Choose version, currently ${versionName(selected)}`}
                    aria-haspopup="menu"
                    aria-expanded={menuOpen}
                    onClick={() => setMenuOpen(!menuOpen)}
                  >
                    {shortName(selected)}
                    <ChevronDownIcon />
                  </button>
                )}
                {multiple && menuOpen && (
                  <div role="menu" aria-label="Versions" className={styles.menu}>
                    {present.map((f) => (
                      <button
                        key={f.file_id}
                        type="button"
                        role="menuitemradio"
                        aria-checked={f.file_id === selectedId}
                        className={styles.menuItem}
                        onClick={() => {
                          setPicked(f.file_id)
                          setMenuOpen(false)
                        }}
                      >
                        <span>{versionName(f)}</span>
                        <span className={styles.size}>{bytes(f.size)}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
              <button
                type="button"
                className={detail.secondary}
                onClick={() => api.setPlayed(m.id, !played).then(movie.reload)}
              >
                {played ? 'Mark unwatched' : 'Mark watched'}
              </button>
            </>
          ) : (
            <span className={detail.notice}>
              {m.availability === 'missing' ? 'File offline' : 'Not in library'}
            </span>
          )}
          {!folderVideo && (
            <button
              type="button"
              className={detail.round}
              aria-label="Add to collection"
              disabled
              title="Collections aren't built yet"
            >
              <PlusIcon />
            </button>
          )}
        </div>
      )}
      {notice && (
        <p className={detail.notice} role="status">
          {notice}
        </p>
      )}
      {manage && m.refresh_error && (
        <p className={detail.notice}>Last refresh failed: {m.refresh_error}</p>
      )}

      <section className={detail.featured} aria-labelledby="versions-title">
        <h2 id="versions-title" className={detail.featuredTitle}>
          {folderVideo ? 'File' : manage ? 'Files' : 'Versions'}
        </h2>
        {m.files.length === 0 && <p className={detail.notice}>No files linked yet.</p>}
        {files.map((f) => {
          const available = f.status === 'present'
          const on = available && f.file_id === selectedId
          const name = folderVideo ? f.relative_path.split('/').pop() : versionName(f)
          const body = (
            <>
              <span className={styles.versionText}>
                <span className={styles.versionName}>{name}</span>
                {available ? (
                  <span className={styles.chips}>
                    {chips(
                      f,
                      versions.data?.find((v) => v.file_id === f.file_id),
                    ).map((c) => (
                      <span key={c} className={styles.chip}>
                        {c}
                      </span>
                    ))}
                  </span>
                ) : (
                  <span className={styles.gone}>
                    File {f.status} · last seen {ago(f.last_seen)}
                  </span>
                )}
                {manage && (
                  <span className={styles.path}>
                    {f.root_path}/{f.relative_path} · {matchSummary([f])}
                  </span>
                )}
                {manage && <Streams fileId={f.file_id} />}
              </span>
              {available && <span className={styles.size}>{bytes(f.size)}</span>}
            </>
          )
          if (manage || !multiple || !available) {
            return (
              <div
                key={f.file_id}
                className={`${styles.version} ${on && !manage ? styles.on : ''}`}
              >
                {body}
              </div>
            )
          }
          return (
            <label key={f.file_id} className={`${styles.version} ${on ? styles.on : ''}`}>
              <input
                type="radio"
                name="version"
                className={styles.radio}
                checked={on}
                onChange={() => setPicked(f.file_id)}
              />
              {body}
            </label>
          )
        })}
      </section>
    </DetailHeader>
  )
}
