package app.mediacabinet.core.playback

import org.junit.Assert.assertEquals
import org.junit.Test

// The same cases as web/src/core/subtitles.test.ts.
class SubtitlesTest {
    private val vtt = """WEBVTT

NOTE a comment --- not a cue

1
00:00:01.500 --> 00:00:03.000 align:start
<i>Two</i> lines &amp; markup
of text

00:04.000 --> 00:05.000
No hours or number

01:00:00.000 --> 01:00:01.000
An hour in
"""

    @Test
    fun `reads cues, with or without numbers and hours`() {
        assertEquals(
            listOf(
                Cue(1.5, 3.0, "Two lines & markup\nof text"),
                Cue(4.0, 5.0, "No hours or number"),
                Cue(3600.0, 3601.0, "An hour in"),
            ),
            parseVtt(vtt),
        )
    }

    @Test
    fun `copes with Windows line endings and a cue with no text`() {
        assertEquals(
            listOf(Cue(1.0, 2.0, "Hi")),
            parseVtt("WEBVTT\r\n\r\n00:01.000 --> 00:02.000\r\nHi\r\n\r\n00:03.000 --> 00:04.000\r\n"),
        )
    }

    @Test
    fun `finds what is on screen, including overlaps`() {
        val cues = listOf(Cue(1.0, 3.0, "one"), Cue(2.0, 4.0, "two"))
        assertEquals("", cues.textAt(0.5))
        assertEquals("one", cues.textAt(1.0))
        assertEquals("one\ntwo", cues.textAt(2.5))
        assertEquals("two", cues.textAt(3.0))
        assertEquals("", cues.textAt(4.0))
    }
}
