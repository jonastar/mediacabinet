import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parseVtt, textAt } from './subtitles.ts'

const VTT = `WEBVTT

NOTE a comment --- not a cue

1
00:00:01.500 --> 00:00:03.000 align:start
<i>Two</i> lines &amp; markup
of text

00:04.000 --> 00:05.000
No hours or number

01:00:00.000 --> 01:00:01.000
An hour in
`

test('reads cues, with or without numbers and hours', () => {
  assert.deepEqual(parseVtt(VTT), [
    { start: 1.5, end: 3, text: 'Two lines & markup\nof text' },
    { start: 4, end: 5, text: 'No hours or number' },
    { start: 3600, end: 3601, text: 'An hour in' },
  ])
})

test('copes with Windows line endings and a cue with no text', () => {
  assert.deepEqual(
    parseVtt('WEBVTT\r\n\r\n00:01.000 --> 00:02.000\r\nHi\r\n\r\n00:03.000 --> 00:04.000\r\n'),
    [{ start: 1, end: 2, text: 'Hi' }],
  )
})

test('finds what is on screen, including overlaps', () => {
  const cues = [
    { start: 1, end: 3, text: 'one' },
    { start: 2, end: 4, text: 'two' },
  ]
  assert.equal(textAt(cues, 0.5), '')
  assert.equal(textAt(cues, 1), 'one')
  assert.equal(textAt(cues, 2.5), 'one\ntwo')
  assert.equal(textAt(cues, 3), 'two')
  assert.equal(textAt(cues, 4), '')
})
