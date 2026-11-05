import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { components } from '../api/schema.d.ts'
import { nextFailure, without, worthRemembering, type Failure } from './fallback.ts'

type ClientCaps = components['schemas']['ClientCaps']
type PlayPlan = components['schemas']['PlayPlan']
type Output = NonNullable<PlayPlan['output']>

const copied: Output = {
  video_codec: 'hevc',
  video_encoder: null,
  audio_codec: 'ac3',
  audio_bitrate_kbps: null,
  segment_seconds: 4,
  tonemap: false,
  burned_subtitle: false,
}

function plan(mode: PlayPlan['mode'], output: Output | null, hdr: string | null = null): PlayPlan {
  return {
    mode,
    output,
    version: { container: 'mkv', hdr_format: hdr },
  } as PlayPlan
}

const caps: ClientCaps = {
  containers: ['mp4', 'mkv'],
  video_codecs: ['h264', 'hevc'],
  audio_codecs: ['aac', 'ac3'],
  hdr: true,
}

test('direct play gives up the container first', () => {
  assert.deepEqual(nextFailure(plan('direct', null), caps), { kind: 'containers', value: 'mkv' })
})

test('steps down one thing at a time, to what the server encodes itself', () => {
  const server = (c: ClientCaps): PlayPlan => {
    const hevcOk = c.video_codecs?.includes('hevc')
    const hdrOk = c.hdr_formats?.length ? c.hdr_formats.includes('hdr10') : c.hdr
    const copyVideo = hevcOk && hdrOk
    const copyAudio = c.audio_codecs?.includes('ac3')
    return plan(
      copyVideo ? 'remux' : 'transcode',
      {
        ...copied,
        video_codec: copyVideo ? 'hevc' : 'h264',
        video_encoder: copyVideo ? null : 'libx264',
        audio_codec: copyAudio ? 'ac3' : 'aac',
        audio_bitrate_kbps: copyAudio ? null : 192,
      },
      'hdr10',
    )
  }
  const seen: Failure[] = []
  let current = caps
  for (;;) {
    const failure = nextFailure(server(current), current)
    if (!failure) {
      break
    }
    seen.push(failure)
    current = without(current, failure)
  }
  assert.deepEqual(seen, [
    { kind: 'audio_codecs', value: 'ac3' },
    { kind: 'hdr_formats', value: 'hdr10' },
  ])
  // HDR off alone sends it through the encoder, so HEVC itself stays allowed.
  assert.ok(current.video_codecs?.includes('hevc'))
})

test('SDR video gives up its codec', () => {
  const noAudio = { ...copied, audio_codec: 'aac', audio_bitrate_kbps: 192 }
  assert.deepEqual(nextFailure(plan('remux', noAudio), caps), {
    kind: 'video_codecs',
    value: 'hevc',
  })
})

test('nothing is left once the server encodes both', () => {
  const encoded = {
    ...copied,
    video_codec: 'h264',
    video_encoder: 'libx264',
    audio_codec: 'aac',
    audio_bitrate_kbps: 192,
  }
  assert.equal(nextFailure(plan('transcode', encoded), caps), null)
})

test('a stream with no audio goes straight to the video', () => {
  const silent = { ...copied, audio_codec: null }
  assert.equal(nextFailure(plan('remux', silent), caps)?.kind, 'video_codecs')
})

test('copied H.264 and AAC can still be ruled out, but are not remembered', () => {
  const basic = { ...copied, video_codec: 'h264', audio_codec: 'aac' }
  const audio = nextFailure(plan('remux', basic), caps)
  assert.deepEqual(audio, { kind: 'audio_codecs', value: 'aac' })
  assert.equal(worthRemembering(audio as Failure), false)
  assert.equal(worthRemembering({ kind: 'video_codecs', value: 'h264' }), false)
  assert.equal(worthRemembering({ kind: 'video_codecs', value: 'hevc' }), true)
})

test('ruling out one HDR format keeps the others', () => {
  const next = without(caps, { kind: 'hdr_formats', value: 'dolby_vision' })
  assert.deepEqual(next.hdr_formats, ['hdr10', 'hlg'])
  assert.equal(next.hdr, true)
})

test('ruling out the last HDR format turns HDR off', () => {
  const next = without(
    { ...caps, hdr_formats: ['hlg'] },
    {
      kind: 'hdr_formats',
      value: 'hlg',
    },
  )
  assert.deepEqual(next.hdr_formats, [])
  assert.equal(next.hdr, false)
})

test('ruling out leaves the original alone', () => {
  const next = without(caps, { kind: 'video_codecs', value: 'HEVC' })
  assert.deepEqual(next.video_codecs, ['h264'])
  assert.deepEqual(caps.video_codecs, ['h264', 'hevc'])
})
