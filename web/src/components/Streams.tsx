import { useState } from 'react'
import { api, type StoredProbe, type StoredStream } from '../api/client'
import { useApi } from '../api/useApi'
import { ago } from '../lib/format'
import { useLiveEvents } from '../lib/live'
import { channelLayout, clock, codecName, language } from '../core/labels'
import styles from './Streams.module.css'

const HDR: Record<string, string> = { hdr10: 'HDR10', hlg: 'HLG', dolby_vision: 'Dolby Vision' }

function bitrate(bps: number | null | undefined) {
  if (!bps) {
    return null
  }
  return bps >= 1e6 ? `${(bps / 1e6).toFixed(1)} Mb/s` : `${Math.round(bps / 1e3)} kb/s`
}

/** `3840×2160 · HDR10 · 18.2 Mb/s`, `5.1 · 640 kb/s`. */
function details(s: StoredStream) {
  const size = s.width && s.height ? `${s.width}×${s.height}` : null
  const hdr = s.hdr_format ? (HDR[s.hdr_format] ?? s.hdr_format) : null
  return [size, hdr, channelLayout(s.channels), bitrate(s.bitrate)].filter(Boolean).join(' · ')
}

function summary(p: StoredProbe) {
  const parts = [p.container, p.duration_seconds ? clock(p.duration_seconds) : null]
  parts.push(p.probed_at ? `probed ${ago(p.probed_at)}` : 'not probed yet')
  return parts.filter(Boolean).join(' · ')
}

function Probe({ fileId }: { fileId: number }) {
  const probe = useApi(`file-probe:${fileId}`, () => api.fileProbe(fileId))
  useLiveEvents(['file.changed', 'file.upgraded', 'scan.finished'], probe.reload)
  if (probe.error) {
    return <p className={styles.error}>{probe.error.message}</p>
  }
  if (!probe.data) {
    return <p className={styles.note}>Loading…</p>
  }
  const p = probe.data
  return (
    <div className={styles.probe}>
      <p className={styles.note}>{summary(p)}</p>
      {p.probe_error && <p className={styles.error}>Probe failed: {p.probe_error}</p>}
      {p.streams.length > 0 && (
        <table className={styles.table}>
          <thead>
            <tr>
              <th scope="col">#</th>
              <th scope="col">Kind</th>
              <th scope="col">Codec</th>
              <th scope="col">Language</th>
              <th scope="col">Details</th>
              <th scope="col">Flags</th>
            </tr>
          </thead>
          <tbody>
            {p.streams.map((s) => (
              <tr key={s.index}>
                <td>{s.index}</td>
                <td>{s.kind}</td>
                <td title={s.codec ?? undefined}>{codecName(s.codec) ?? '—'}</td>
                <td title={s.language ?? undefined}>{language(s.language) ?? '—'}</td>
                <td>{details(s) || '—'}</td>
                <td>
                  {[s.is_default && 'default', s.is_forced && 'forced'].filter(Boolean).join(', ')}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {p.sidecars.length > 0 && (
        <ul className={styles.sidecars}>
          {p.sidecars.map((s) => (
            <li key={s.id}>
              {[
                s.kind,
                s.relative_path,
                language(s.language),
                s.is_forced && 'forced',
                s.is_hearing_impaired && 'SDH',
                s.location === 'cache' && 'cached',
              ]
                .filter(Boolean)
                .join(' · ')}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** A file's probed streams and sidecars, behind a toggle so they're fetched only when wanted. */
export function Streams({ fileId }: { fileId: number }) {
  const [open, setOpen] = useState(false)
  return (
    <div className={styles.streams}>
      <button
        type="button"
        className={styles.toggle}
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        {open ? 'Hide streams' : 'Streams'}
      </button>
      {open && <Probe fileId={fileId} />}
    </div>
  )
}
