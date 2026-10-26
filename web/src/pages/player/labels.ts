import type { Version } from '../../api/client'
import { bytes, resolution } from '../../lib/format'

let names: Intl.DisplayNames | null = null
try {
  names = new Intl.DisplayNames(undefined, { type: 'language' })
} catch {
  names = null
}

/** `eng` → `English`; unknown codes as they are. */
export function language(code: string | null | undefined): string | null {
  if (!code || code === 'und') {
    return null
  }
  try {
    return names?.of(code) ?? code
  } catch {
    return code
  }
}

const CODECS: Record<string, string> = {
  aac: 'AAC',
  ac3: 'Dolby Digital',
  eac3: 'Dolby Digital+',
  truehd: 'TrueHD',
  dts: 'DTS',
  flac: 'FLAC',
  opus: 'Opus',
  mp3: 'MP3',
  subrip: 'SRT',
  srt: 'SRT',
  ass: 'ASS',
  ssa: 'SSA',
  webvtt: 'WebVTT',
  vtt: 'WebVTT',
  mov_text: 'Text',
  hdmv_pgs_subtitle: 'PGS',
  dvd_subtitle: 'DVD',
  hevc: 'HEVC',
  h264: 'H.264',
  av1: 'AV1',
  vp9: 'VP9',
}

export const codecName = (codec: string | null | undefined) =>
  codec ? (CODECS[codec] ?? codec.toUpperCase()) : null

function channelLayout(channels: number | null | undefined) {
  switch (channels) {
    case 1:
      return 'Mono'
    case 2:
      return 'Stereo'
    case 6:
      return '5.1'
    case 8:
      return '7.1'
    default:
      return channels ? `${channels} ch` : null
  }
}

export function audioLabel(track: Version['audio'][number], index: number): string {
  const parts = [language(track.language), codecName(track.codec), channelLayout(track.channels)]
  const label = parts.filter(Boolean).join(' · ')
  return label || `Track ${index + 1}`
}

export function subtitleLabel(track: Version['subtitles'][number], index: number): string {
  const name = language(track.language) ?? `Track ${index + 1}`
  const notes = [
    track.forced && 'forced',
    track.hearing_impaired && 'SDH',
    !track.text && 'burned in',
    track.external && 'file',
  ].filter(Boolean)
  return notes.length ? `${name} (${notes.join(', ')})` : name
}

export function versionLabel(v: Version): string {
  const parts = [
    resolution(v.height),
    v.hdr_format ? v.hdr_format.toUpperCase().replace('_', ' ') : null,
    codecName(v.video_codec),
    bytes(v.size),
  ]
  return parts.filter(Boolean).join(' · ')
}

/** `1:02:03` or `4:05`. */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = String(s % 60).padStart(2, '0')
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`
}
