package app.mediacabinet.core.playback

import app.mediacabinet.core.api.ClientCaps
import app.mediacabinet.core.api.Mode
import app.mediacabinet.core.api.PlayPlan
import kotlinx.serialization.Serializable

// A port of web/src/core/fallback.ts, which is the reference: keep them in step.

/**
 * Something the device said it could play but couldn't. Capabilities are only a
 * first guess, so when playback fails to start the player asks again without one
 * thing at a time, until the server is sending H.264 and AAC it encoded itself.
 * What worked is remembered for the device (`docs/clients.md`).
 */
@Serializable
data class Failure(val kind: Kind, val value: String) {
    enum class Kind(val label: String) {
        Containers("container"),
        VideoCodecs("video"),
        AudioCodecs("audio"),
        HdrFormats("HDR"),
    }

    /** In words, for the stats panel. */
    fun describe() = "$value (${kind.label})"

    /**
     * Whether it says something about the device and not just one file. H.264 and
     * AAC only get ruled out to force a file through the encoder, as with 10-bit
     * H.264, which most devices can't decode.
     */
    fun worthRemembering() = BASELINE.none { it.equals(value, ignoreCase = true) }
}

private val HDR_FORMATS = listOf("hdr10", "hlg", "dolby_vision")
private val BASELINE = listOf("h264", "aac")

/** The HDR formats these capabilities cover; `hdr` alone means all of them. */
private fun ClientCaps.coveredHdr(): List<String> = when {
    !hdrFormats.isNullOrEmpty() -> hdrFormats
    hdr == true -> HDR_FORMATS
    else -> emptyList()
}

/**
 * What to rule out next, after `plan` failed, or null if there's nothing left to
 * try. The least costly goes first: a file that can be repackaged shouldn't be
 * converted, and the audio can change without touching the video.
 */
fun nextFailure(plan: PlayPlan, caps: ClientCaps): Failure? {
    if (plan.mode == Mode.Direct) {
        return plan.version.container?.let { Failure(Failure.Kind.Containers, it) }
    }
    val output = plan.output ?: return null
    val audioCodec = output.audioCodec
    if (audioCodec != null && output.audioBitrateKbps == null) {
        // Copied as it is, so it's the device's to decode.
        return Failure(Failure.Kind.AudioCodecs, audioCodec)
    }
    if (output.videoEncoder != null) {
        return null
    }
    val hdr = plan.version.hdrFormat
    if (hdr != null && caps.coveredHdr().any { it.equals(hdr, ignoreCase = true) }) {
        return Failure(Failure.Kind.HdrFormats, hdr)
    }
    return output.videoCodec?.let { Failure(Failure.Kind.VideoCodecs, it) }
}

/** These capabilities with a failure ruled out. */
fun ClientCaps.without(failure: Failure): ClientCaps {
    val drop = { list: List<String>? -> (list ?: emptyList()).filterNot { it.equals(failure.value, ignoreCase = true) } }
    return when (failure.kind) {
        Failure.Kind.HdrFormats -> {
            val formats = drop(coveredHdr())
            copy(hdrFormats = formats, hdr = formats.isNotEmpty())
        }
        Failure.Kind.Containers -> copy(containers = drop(containers))
        Failure.Kind.VideoCodecs -> copy(videoCodecs = drop(videoCodecs))
        Failure.Kind.AudioCodecs -> copy(audioCodecs = drop(audioCodecs))
    }
}
