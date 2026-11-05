import type { components } from '../api/schema.d.ts'

type ClientCaps = components['schemas']['ClientCaps']
type PlayPlan = components['schemas']['PlayPlan']

/**
 * Something a player said it could play but couldn't. Capabilities are only a
 * first guess, so when playback fails to start the player asks again without
 * one thing at a time, until the server is sending H.264 and AAC it encoded
 * itself. What worked is remembered for the device (`docs/clients.md`).
 */
export interface Failure {
  kind: 'containers' | 'video_codecs' | 'audio_codecs' | 'hdr_formats'
  value: string
}

const HDR_FORMATS = ['hdr10', 'hlg', 'dolby_vision']

/** Too basic to rule out for every file because one of them failed. */
const BASELINE = ['h264', 'aac']

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase()

/** The HDR formats these capabilities cover; `hdr` alone means all of them. */
function hdrFormats(caps: ClientCaps): string[] {
  if (caps.hdr_formats?.length) {
    return caps.hdr_formats
  }
  return caps.hdr ? HDR_FORMATS : []
}

/**
 * What to rule out next, after `plan` failed, or null if there's nothing left
 * to try. The least costly goes first: a file that can be repackaged shouldn't
 * be converted, and the audio can change without touching the video.
 */
export function nextFailure(plan: PlayPlan, caps: ClientCaps): Failure | null {
  if (plan.mode === 'direct') {
    return plan.version.container ? { kind: 'containers', value: plan.version.container } : null
  }
  const output = plan.output
  if (!output) {
    return null
  }
  const audioCopied = output.audio_codec && !output.audio_bitrate_kbps
  if (audioCopied) {
    return { kind: 'audio_codecs', value: output.audio_codec as string }
  }
  if (output.video_encoder) {
    return null
  }
  const hdr = plan.version.hdr_format
  if (hdr && hdrFormats(caps).some((format) => same(format, hdr))) {
    return { kind: 'hdr_formats', value: hdr }
  }
  return output.video_codec ? { kind: 'video_codecs', value: output.video_codec } : null
}

/** `caps` with a failure ruled out. */
export function without(caps: ClientCaps, failure: Failure): ClientCaps {
  const { kind, value } = failure
  if (kind === 'hdr_formats') {
    const formats = hdrFormats(caps).filter((format) => !same(format, value))
    return { ...caps, hdr_formats: formats, hdr: formats.length > 0 }
  }
  return { ...caps, [kind]: (caps[kind] ?? []).filter((item) => !same(item, value)) }
}

/**
 * Whether a failure says something about the device and not just one file.
 * H.264 and AAC only get ruled out to force a file through the encoder, as with
 * 10-bit H.264, which most devices can't decode.
 */
export function worthRemembering(failure: Failure): boolean {
  return !BASELINE.some((value) => same(value, failure.value))
}

const KIND_LABEL: Record<Failure['kind'], string> = {
  containers: 'container',
  video_codecs: 'video',
  audio_codecs: 'audio',
  hdr_formats: 'HDR',
}

/** A failure in words, for stats panels. */
export function describeFailure(failure: Failure): string {
  return `${failure.value} (${KIND_LABEL[failure.kind]})`
}
