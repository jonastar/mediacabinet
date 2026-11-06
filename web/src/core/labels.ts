import type { components } from '../api/schema.d.ts'

type Version = components['schemas']['Version']

let names: Intl.DisplayNames | null = null
try {
  names = new Intl.DisplayNames(undefined, { type: 'language' })
} catch {
  // Older engines have no language names: see COMMON_LANGUAGES.
  names = null
}

/** For engines without `Intl.DisplayNames`, which is most TVs before 2021. */
const COMMON_LANGUAGES: Record<string, string> = {
  ara: 'Arabic',
  chi: 'Chinese',
  zho: 'Chinese',
  cze: 'Czech',
  ces: 'Czech',
  dan: 'Danish',
  dut: 'Dutch',
  nld: 'Dutch',
  eng: 'English',
  fin: 'Finnish',
  fre: 'French',
  fra: 'French',
  ger: 'German',
  deu: 'German',
  gre: 'Greek',
  ell: 'Greek',
  heb: 'Hebrew',
  hin: 'Hindi',
  hun: 'Hungarian',
  ita: 'Italian',
  jpn: 'Japanese',
  kor: 'Korean',
  nor: 'Norwegian',
  pol: 'Polish',
  por: 'Portuguese',
  rus: 'Russian',
  spa: 'Spanish',
  swe: 'Swedish',
  tur: 'Turkish',
  ukr: 'Ukrainian',
}

/** `eng` → `English`; unknown codes as they are. */
export function language(code: string | null | undefined): string | null {
  if (!code || code === 'und') {
    return null
  }
  try {
    return names?.of(code) ?? COMMON_LANGUAGES[code] ?? code
  } catch {
    return COMMON_LANGUAGES[code] ?? code
  }
}

export function bytes(n: number): string {
  if (n >= 1e9) {
    return `${(n / 1e9).toFixed(1)} GB`
  }
  if (n >= 1e6) {
    return `${Math.round(n / 1e6)} MB`
  }
  return `${Math.max(1, Math.round(n / 1e3))} KB`
}

export function resolution(height: number | null | undefined): string | null {
  if (!height) {
    return null
  }
  const steps = [2160, 1440, 1080, 720, 576, 480]
  const step = steps.find((s) => height >= s * 0.9)
  return `${step ?? height}p`
}

/** The heights a stream can be converted down to; `null` is the file as it is. */
export const QUALITIES: [number | null, string][] = [
  [null, 'Original'],
  [1080, '1080p'],
  [720, '720p'],
  [480, '480p'],
]

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

export function channelLayout(channels: number | null | undefined) {
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
