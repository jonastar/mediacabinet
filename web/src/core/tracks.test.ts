import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { components } from '../api/schema.d.ts'
import { needsNewStream, subtitleUrl } from './tracks.ts'

type PlayPlan = components['schemas']['PlayPlan']

const track = (id: string, text: boolean) => ({
  id,
  text,
  external: false,
  forced: false,
  hearing_impaired: false,
})

function plan(burned = false) {
  return {
    version: { subtitles: [track('stream:2', true), track('stream:3', false)] },
    subtitles: [
      { id: 'stream:2', url: '/subs/2?offset=0', forced: false, hearing_impaired: false },
      ...(burned ? [{ id: 'stream:3', url: null, forced: false, hearing_impaired: false }] : []),
    ],
  } as unknown as PlayPlan
}

const from = { file_id: 1, audio_stream: 1, subtitle: null }

test('text subtitles change without a new stream', () => {
  assert.equal(needsNewStream(plan(), from, { ...from, subtitle: 'stream:2' }), false)
  assert.equal(needsNewStream(plan(), { ...from, subtitle: 'stream:2' }, from), false)
})

test('picture subtitles need a new stream both ways', () => {
  assert.equal(needsNewStream(plan(), from, { ...from, subtitle: 'stream:3' }), true)
  assert.equal(needsNewStream(plan(true), { ...from, subtitle: 'stream:3' }, from), true)
})

test('another file, audio track or quality needs a new stream', () => {
  assert.equal(needsNewStream(plan(), from, { ...from, file_id: 2 }), true)
  assert.equal(needsNewStream(plan(), from, { ...from, audio_stream: 2 }), true)
  assert.equal(needsNewStream(plan(), from, { ...from, max_height: 720 }), true)
})

test('unset and null mean the same', () => {
  assert.equal(needsNewStream(plan(), from, { ...from, max_height: null }), false)
})

test('only subtitles with a URL are drawn', () => {
  assert.equal(subtitleUrl(plan(), { subtitle: 'stream:2' }), '/subs/2?offset=0')
  assert.equal(subtitleUrl(plan(true), { subtitle: 'stream:3' }), null)
  assert.equal(subtitleUrl(plan(), {}), null)
})
