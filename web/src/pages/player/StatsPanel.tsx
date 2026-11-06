import { useState } from 'react'
import type { PlayPlan } from '../../api/client'
import { bytes } from '../../lib/format'
import { describeFailure } from '../../core/fallback'
import { clientCaps, forgetFailures, rememberedFailures } from '../../core/caps'
import { audioLabel, channelLayout, clock, codecName } from '../../core/labels'
import type { Step } from './meter'
import type { Snapshot } from './useStats'
import styles from './Player.module.css'

interface Section {
  title: string
  rows: [string, string][]
  /** Long text shown as it is: the ffmpeg command, its log. */
  blocks?: [string, string][]
}

const MODES: Record<PlayPlan['mode'], string> = {
  direct: 'Direct play',
  remux: 'Repackaging',
  transcode: 'Converting',
}

/** `12.4 Mbit/s`, `850 kbit/s`. */
function rate(bps: number | null | undefined): string {
  if (bps === null || bps === undefined || !Number.isFinite(bps)) {
    return '—'
  }
  if (bps >= 1e6) {
    return `${(bps / 1e6).toFixed(1)} Mbit/s`
  }
  return `${Math.round(bps / 1e3)} kbit/s`
}

function seconds(ms: number | null): string {
  if (ms === null) {
    return '—'
  }
  return ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(1)} s`
}
const size = (s: { width: number; height: number } | null) => (s ? `${s.width}×${s.height}` : '—')

const STEPS: Record<Step, string> = {
  planned: 'Server answered',
  hls: 'hls.js ready',
  playlist: 'Playlist loaded',
  segment: 'First segment loaded',
  metadata: 'Metadata read',
  frame: 'First frame',
}

/** Each step of starting the stream and how long it took after the one before. */
function startupSection(s: Snapshot): Section {
  const reached = (Object.entries(s.startup) as [Step, number][]).sort((a, b) => a[1] - b[1])
  const ffmpeg = s.server?.first_segment_seconds
  const rows = reached.map(([step, at], i): [string, string] => {
    if (i === 0) {
      const note =
        step === 'planned' && ffmpeg ? ` (ffmpeg's first segment ${seconds(ffmpeg * 1000)})` : ''
      return [STEPS[step], `${seconds(at)}${note}`]
    }
    const after = `+${seconds(at - reached[i - 1][1])}`
    return [STEPS[step], step === 'frame' ? `${after}, ${seconds(at)} in all` : after]
  })
  return { title: 'Startup', rows: rows.length > 0 ? rows : [['Waiting', '—']] }
}

/** What the server sends for an HLS stream, to read against the source. */
function outputSection(plan: PlayPlan): Section[] {
  const out = plan.output
  if (!out) {
    return []
  }
  const v = plan.version
  const copied = out.video_encoder === null || out.video_encoder === undefined
  const video = copied
    ? `${codecName(out.video_codec) ?? '—'} (copied)`
    : [
        codecName(out.video_codec),
        out.height !== null && out.height !== undefined && `${out.height}p`,
        out.max_bitrate_kbps && `up to ${rate(out.max_bitrate_kbps * 1000)}`,
        out.video_encoder,
      ]
        .filter(Boolean)
        .join(' · ')
  const changes = copied
    ? []
    : [
        out.height !== v.height && `scaled from ${v.height}p`,
        out.tonemap && `HDR tone-mapped to SDR`,
        out.burned_subtitle && 'subtitles burned in',
      ].filter((c): c is string => Boolean(c))
  const audio = [
    codecName(out.audio_codec),
    channelLayout(out.audio_channels),
    out.audio_bitrate_kbps ? rate(out.audio_bitrate_kbps * 1000) : '(copied)',
  ]
    .filter(Boolean)
    .join(' · ')
  return [
    {
      title: 'Output',
      rows: [
        ['Video', video],
        ...(changes.length > 0 ? [['Changes', changes.join(', ')] as [string, string]] : []),
        ['Audio', out.audio_codec ? audio : 'None'],
        [
          'Container',
          `HLS, fMP4 segments of ${out.segment_seconds} s${
            out.initial_segment_seconds
              ? ` (${out.initial_segment_seconds} s for the first ${out.segment_seconds} s)`
              : ''
          }`,
        ],
      ],
    },
  ]
}

function sections(plan: PlayPlan, s: Snapshot): Section[] {
  const v = plan.version
  const offset = plan.offset_seconds
  const audio = v.audio.find((a) => a.index === plan.audio_stream)
  const fileBitrate =
    v.duration_seconds && v.duration_seconds > 0 ? (v.size * 8) / v.duration_seconds : null
  const how = plan.mode === 'transcode' ? `${MODES.transcode} (${plan.encoder})` : MODES[plan.mode]

  const result: Section[] = [
    {
      title: 'Stream',
      rows: [
        ['Mode', how],
        ...plan.reasons.map((r): [string, string] => ['Why', r]),
        ['Starts at', clock(offset)],
      ],
    },
    {
      title: 'Source',
      rows: [
        ['File', v.file_name],
        ['Container', `${v.container ?? '—'} · ${bytes(v.size)}`],
        [
          'Video',
          [codecName(v.video_codec), v.width && v.height && `${v.width}×${v.height}`, v.hdr_format]
            .filter(Boolean)
            .join(' · ') || '—',
        ],
        ['Audio', audio ? audioLabel(audio, 0) : '—'],
        ['Average bitrate', rate(fileBitrate)],
      ],
    },
    startupSection(s),
    ...outputSection(plan),
    {
      title: 'Playback',
      rows: [
        ['Decoded', size(s.decoded)],
        ['On screen', size(s.display)],
        ...(s.codecs.video || s.codecs.audio
          ? [
              [
                'Codecs',
                [s.codecs.video, s.codecs.audio].filter(Boolean).join(',').replaceAll(',', ', '),
              ] as [string, string],
            ]
          : []),
        [
          'Bitrate',
          `${rate(s.bitrate)}${s.bitrate !== null && s.bitrateAverage ? ' (average)' : ''}`,
        ],
        ['Buffer ahead', `${s.bufferAhead.toFixed(1)} s`],
        [
          'Dropped frames',
          s.frames > 0
            ? `${s.dropped} of ${s.frames} (${((s.dropped / s.frames) * 100).toFixed(1)}%)`
            : '—',
        ],
        ['Stalls', s.stalls > 0 ? `${s.stalls} (${seconds(s.stalledMs)})` : 'None'],
      ],
    },
    {
      title: 'Network',
      rows: [
        [
          'Download rate',
          s.download === null
            ? '—'
            : s.downloadEstimated && s.download <= 0
              ? 'Not downloading'
              : s.downloadEstimated
                ? `≈ ${rate(s.download)} (from buffer growth)`
                : `${rate(s.download)} (last segments)`,
        ],
        ...(s.hlsEstimate !== null
          ? [['Bandwidth estimate', rate(s.hlsEstimate)] as [string, string]]
          : []),
      ],
    },
  ]

  const server = s.server
  if (server) {
    const ahead = server.converted_seconds - s.time
    const state = server.finished
      ? 'Finished'
      : server.throttled
        ? 'Paused, far enough ahead'
        : 'Running'
    const blocks: [string, string][] = []
    if (server.command) {
      blocks.push(['Command', server.command])
    }
    if (server.log) {
      blocks.push(['Log', server.log])
    }
    result.push({
      title: 'Server',
      rows: [
        ['ffmpeg', state],
        ['Speed', server.speed !== null && server.speed !== undefined ? `${server.speed}×` : '—'],
        ['Frame rate', server.fps ? `${server.fps.toFixed(1)} fps` : '—'],
        ['Output bitrate', server.bitrate_kbps ? rate(server.bitrate_kbps * 1000) : '—'],
        [
          'Converted to',
          `${clock(offset + server.converted_seconds)} (${Math.max(ahead, 0).toFixed(0)} s ahead)`,
        ],
        ['Segments', `${server.segments_written} written, ${server.segments_requested} asked for`],
        ['Running for', clock(server.running_seconds)],
        ['Session', plan.session_id ?? '—'],
      ],
      blocks,
    })
  }

  const caps = clientCaps()
  result.push({
    title: 'Browser',
    rows: [
      ['Video codecs', (caps.video_codecs ?? []).join(', ') || 'None'],
      ['Audio codecs', (caps.audio_codecs ?? []).join(', ') || 'None'],
      ['Containers', (caps.containers ?? []).join(', ') || 'None'],
      ['HDR', caps.hdr ? 'Yes' : 'No'],
      ['Found not to play', rememberedFailures().map(describeFailure).join(', ') || 'Nothing'],
      ['User agent', navigator.userAgent],
    ],
  })
  return result
}

function asText(list: Section[]): string {
  return list
    .map((section) =>
      [
        `## ${section.title}`,
        ...section.rows.map(([label, value]) => `${label}: ${value}`),
        ...(section.blocks ?? []).map(([label, text]) => `${label}:\n${text}`),
      ].join('\n'),
    )
    .join('\n\n')
}

/** Stats for nerds: how the stream is sent, downloaded and played. */
export function StatsPanel({
  plan,
  snapshot,
  onClose,
}: {
  plan: PlayPlan
  snapshot: Snapshot | null
  onClose: () => void
}) {
  const [copied, setCopied] = useState(false)
  const [, setForgotten] = useState(0)
  if (!snapshot) {
    return null
  }
  const list = sections(plan, snapshot)
  return (
    <aside className={styles.stats} aria-label="Playback stats">
      <div className={styles.statsHead}>
        <b>Playback stats</b>
        <button
          type="button"
          className={styles.statsButton}
          onClick={() =>
            navigator.clipboard?.writeText(asText(list)).then(() => {
              setCopied(true)
              window.setTimeout(() => setCopied(false), 1500)
            })
          }
        >
          {copied ? 'Copied' : 'Copy'}
        </button>
        {rememberedFailures().length > 0 && (
          <button
            type="button"
            className={styles.statsButton}
            title="Try everything this browser says it can play again"
            onClick={() => {
              forgetFailures()
              setForgotten((n) => n + 1)
            }}
          >
            Forget failures
          </button>
        )}
        <button type="button" className={styles.statsButton} onClick={onClose}>
          Close
        </button>
      </div>
      {list.map((section) => (
        <section key={section.title} className={styles.statsSection}>
          <h2>{section.title}</h2>
          <dl>
            {section.rows.map(([label, value], i) => (
              <div key={`${label}${i}`}>
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
          {section.blocks?.map(([label, text]) => (
            <details key={label}>
              <summary>{label}</summary>
              <pre>{text}</pre>
            </details>
          ))}
        </section>
      ))}
    </aside>
  )
}
