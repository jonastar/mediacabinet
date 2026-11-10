package app.mediacabinet.core.playback

import app.mediacabinet.core.api.ClientCaps
import app.mediacabinet.core.api.ItemWatch
import app.mediacabinet.core.api.Mode
import app.mediacabinet.core.api.PlayContext
import app.mediacabinet.core.api.PlayPlan
import app.mediacabinet.core.api.StreamOutput
import app.mediacabinet.core.api.Version
import app.mediacabinet.core.playback.Failure.Kind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// The same cases as web/src/core/fallback.test.ts.
class FallbackTest {
    private val copied = StreamOutput(
        videoCodec = "hevc",
        videoEncoder = null,
        audioCodec = "ac3",
        audioBitrateKbps = null,
        segmentSeconds = 4,
        tonemap = false,
        burnedSubtitle = false,
    )

    private val caps = ClientCaps(
        containers = listOf("mp4", "mkv"),
        videoCodecs = listOf("h264", "hevc"),
        audioCodecs = listOf("aac", "ac3"),
        hdr = true,
    )

    private fun plan(mode: Mode, output: StreamOutput?, hdr: String? = null) = PlayPlan(
        context = PlayContext(itemId = 1, kind = "movie", title = "Film"),
        encoder = "software",
        mode = mode,
        offsetSeconds = 0.0,
        output = output,
        reasons = emptyList(),
        startSeconds = 0.0,
        subtitles = emptyList(),
        url = "/file",
        version = Version(
            audio = emptyList(),
            container = "mkv",
            fileId = 1,
            fileName = "film.mkv",
            hdrFormat = hdr,
            size = 1,
            subtitles = emptyList(),
        ),
        watch = ItemWatch(itemId = 1, playCount = 0, played = false, positionSeconds = 0.0),
    )

    @Test
    fun `direct play gives up the container first`() {
        assertEquals(Failure(Kind.Containers, "mkv"), nextFailure(plan(Mode.Direct, null), caps))
    }

    @Test
    fun `steps down one thing at a time, to what the server encodes itself`() {
        fun server(c: ClientCaps): PlayPlan {
            val hevcOk = c.videoCodecs.orEmpty().contains("hevc")
            val hdrOk = when {
                !c.hdrFormats.isNullOrEmpty() -> "hdr10" in c.hdrFormats
                else -> c.hdr == true
            }
            val copyVideo = hevcOk && hdrOk
            val copyAudio = c.audioCodecs.orEmpty().contains("ac3")
            val output = when {
                copyVideo -> copied.copy(videoCodec = "hevc", videoEncoder = null)
                else -> copied.copy(videoCodec = "h264", videoEncoder = "libx264")
            }
            val withAudio = when {
                copyAudio -> output.copy(audioCodec = "ac3", audioBitrateKbps = null)
                else -> output.copy(audioCodec = "aac", audioBitrateKbps = 192)
            }
            val mode = when {
                copyVideo -> Mode.Remux
                else -> Mode.Transcode
            }
            return plan(mode, withAudio, "hdr10")
        }
        val seen = mutableListOf<Failure>()
        var current = caps
        while (true) {
            val failure = nextFailure(server(current), current) ?: break
            seen += failure
            current = current.without(failure)
        }
        assertEquals(listOf(Failure(Kind.AudioCodecs, "ac3"), Failure(Kind.HdrFormats, "hdr10")), seen)
        // HDR off alone sends it through the encoder, so HEVC itself stays allowed.
        assertTrue("hevc" in current.videoCodecs.orEmpty())
    }

    @Test
    fun `SDR video gives up its codec`() {
        val noAudio = copied.copy(audioCodec = "aac", audioBitrateKbps = 192)
        assertEquals(Failure(Kind.VideoCodecs, "hevc"), nextFailure(plan(Mode.Remux, noAudio), caps))
    }

    @Test
    fun `nothing is left once the server encodes both`() {
        val encoded = copied.copy(videoCodec = "h264", videoEncoder = "libx264", audioCodec = "aac", audioBitrateKbps = 192)
        assertNull(nextFailure(plan(Mode.Transcode, encoded), caps))
    }

    @Test
    fun `a stream with no audio goes straight to the video`() {
        val silent = copied.copy(audioCodec = null)
        assertEquals(Kind.VideoCodecs, nextFailure(plan(Mode.Remux, silent), caps)?.kind)
    }

    @Test
    fun `copied H264 and AAC can still be ruled out, but are not remembered`() {
        val basic = copied.copy(videoCodec = "h264", audioCodec = "aac")
        val audio = nextFailure(plan(Mode.Remux, basic), caps)
        assertEquals(Failure(Kind.AudioCodecs, "aac"), audio)
        assertFalse(audio!!.worthRemembering())
        assertFalse(Failure(Kind.VideoCodecs, "h264").worthRemembering())
        assertTrue(Failure(Kind.VideoCodecs, "hevc").worthRemembering())
    }

    @Test
    fun `ruling out one HDR format keeps the others`() {
        val next = caps.without(Failure(Kind.HdrFormats, "dolby_vision"))
        assertEquals(listOf("hdr10", "hlg"), next.hdrFormats)
        assertEquals(true, next.hdr)
    }

    @Test
    fun `ruling out the last HDR format turns HDR off`() {
        val next = caps.copy(hdrFormats = listOf("hlg")).without(Failure(Kind.HdrFormats, "hlg"))
        assertEquals(emptyList<String>(), next.hdrFormats)
        assertEquals(false, next.hdr)
    }

    @Test
    fun `ruling out leaves the original alone`() {
        val next = caps.without(Failure(Kind.VideoCodecs, "HEVC"))
        assertEquals(listOf("h264"), next.videoCodecs)
        assertEquals(listOf("h264", "hevc"), caps.videoCodecs)
    }
}
