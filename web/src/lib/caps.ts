import type { ClientCaps } from '../api/client'

/** Codec names as the server knows them, with a MIME type to test each. */
const VIDEO: [string, string][] = [
  ['h264', 'video/mp4; codecs="avc1.640028"'],
  // Main 10: most HEVC files are 10-bit, and browsers that only decode 8-bit
  // would otherwise be sent a copy they can't play.
  ['hevc', 'video/mp4; codecs="hvc1.2.4.L153.B0"'],
  ['av1', 'video/mp4; codecs="av01.0.08M.08"'],
  ['vp9', 'video/mp4; codecs="vp09.00.10.08"'],
]

const AUDIO: [string, string][] = [
  ['aac', 'audio/mp4; codecs="mp4a.40.2"'],
  ['mp3', 'audio/mpeg'],
  ['opus', 'audio/mp4; codecs="opus"'],
  ['flac', 'audio/mp4; codecs="flac"'],
  ['ac3', 'audio/mp4; codecs="ac-3"'],
  ['eac3', 'audio/mp4; codecs="ec-3"'],
]

const CONTAINERS: [string, string][] = [
  ['mp4', 'video/mp4'],
  ['webm', 'video/webm'],
  ['mkv', 'video/x-matroska'],
]

function supported(type: string, video: HTMLVideoElement): boolean {
  // Where Media Source exists, HLS (fMP4) goes through it, so it has the final
  // say; a codec only a plain <video> plays would break when copied into HLS.
  const source =
    window.MediaSource ?? (window as { ManagedMediaSource?: typeof MediaSource }).ManagedMediaSource
  if (source) {
    return source.isTypeSupported(type)
  }
  return video.canPlayType(type) === 'probably'
}

let cached: ClientCaps | null = null

/** What this browser can play, for the server to decide how to send a file. */
export function clientCaps(): ClientCaps {
  if (cached) {
    return cached
  }
  const video = document.createElement('video')
  const pick = (list: [string, string][]) =>
    list.filter(([, type]) => supported(type, video)).map(([name]) => name)
  const containers = CONTAINERS.filter(([, type]) => video.canPlayType(type) !== '').map(
    ([name]) => name,
  )
  const videoCodecs = pick(VIDEO)
  const hdrScreen = window.matchMedia?.('(dynamic-range: high)').matches ?? false
  cached = {
    containers,
    video_codecs: videoCodecs,
    audio_codecs: pick(AUDIO),
    // HDR needs a screen for it and a codec that carries it.
    hdr: hdrScreen && (videoCodecs.includes('hevc') || videoCodecs.includes('av1')),
  }
  return cached
}
