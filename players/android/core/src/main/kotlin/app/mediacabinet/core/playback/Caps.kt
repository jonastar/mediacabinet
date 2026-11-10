package app.mediacabinet.core.playback

import android.content.Context
import android.hardware.display.DisplayManager
import android.media.MediaCodecInfo
import android.media.MediaCodecInfo.CodecProfileLevel
import android.media.MediaCodecList
import android.os.Build
import android.view.Display
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.audio.AudioCapabilities
import app.mediacabinet.core.api.ApiJson
import app.mediacabinet.core.api.ClientCaps

/**
 * What this device can play, for the server to decide how to send a file, and
 * what's been found not to play here despite that. Like the web's `caps.ts`.
 */
class Caps(private val context: Context) {
    private val prefs = context.getSharedPreferences("playback", Context.MODE_PRIVATE)
    private val detected by lazy { detect() }

    /** What to ask the server for: what the device says, less what's failed here. */
    fun forPlayback(): ClientCaps = remembered().fold(detected) { caps, failure -> caps.without(failure) }

    /** What's been found not to play here. */
    fun remembered(): List<Failure> = runCatching {
        ApiJson.decodeFromString<List<Failure>>(prefs.getString(FAILURES, null) ?: "[]")
    }.getOrDefault(emptyList())

    /** Keeps failures for next time, so another file skips the failed attempt. */
    fun remember(failures: List<Failure>) {
        val known = remembered()
        val fresh = failures.filter { it.worthRemembering() && it !in known }
        if (fresh.isNotEmpty()) {
            prefs.edit().putString(FAILURES, ApiJson.encodeToString(known + fresh)).apply()
        }
    }

    fun forget() {
        prefs.edit().remove(FAILURES).apply()
    }

    private fun detect(): ClientCaps {
        val decoders = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter { !it.isEncoder }
        val video = VIDEO.filter { (_, mime, profile) -> decoders.any { it.decodes(mime, profile, needsHardware = mime != H264) } }
            .map { it.first }
        val audio = AUDIO.filter { (_, mime) -> decoders.any { it.decodes(mime) } }.map { it.first }
        val passthrough = passthroughAudio().filterNot { it in audio }
        val hdr = hdrFormats(decoders)
        return ClientCaps(
            containers = CONTAINERS,
            videoCodecs = video,
            audioCodecs = audio + passthrough,
            hdr = hdr.isNotEmpty(),
            hdrFormats = hdr,
        )
    }

    /** Codecs a TV or receiver decodes when Media3 passes them through over HDMI. */
    // The replacement takes a list whose meaning Media3 doesn't document yet.
    @Suppress("DEPRECATION")
    @OptIn(UnstableApi::class)
    private fun passthroughAudio(): List<String> {
        val capabilities = AudioCapabilities.getCapabilities(context, AudioAttributes.DEFAULT, null)
        return PASSTHROUGH.filter { (_, encoding) -> capabilities.supportsEncoding(encoding) }.map { it.first }
    }

    /** HDR the screen shows and a decoder carries; otherwise the server tone-maps. */
    private fun hdrFormats(decoders: List<MediaCodecInfo>): List<String> {
        val display = context.getSystemService(DisplayManager::class.java).getDisplay(Display.DEFAULT_DISPLAY)
            ?: return emptyList()
        val types = if (Build.VERSION.SDK_INT >= 34) {
            display.mode.supportedHdrTypes.toList()
        } else {
            @Suppress("DEPRECATION")
            display.hdrCapabilities?.supportedHdrTypes?.toList() ?: emptyList()
        }
        val main10 = decoders.any { it.decodes(HEVC, CodecProfileLevel.HEVCProfileMain10, needsHardware = true) }
        val dolbyVision = decoders.any { it.decodes(DOLBY_VISION, null, needsHardware = true) }
        return buildList {
            if (Display.HdrCapabilities.HDR_TYPE_HDR10 in types && main10) {
                add("hdr10")
            }
            if (Display.HdrCapabilities.HDR_TYPE_HLG in types && main10) {
                add("hlg")
            }
            if (Display.HdrCapabilities.HDR_TYPE_DOLBY_VISION in types && dolbyVision) {
                add("dolby_vision")
            }
        }
    }

    private companion object {
        const val FAILURES = "failures"
        const val H264 = "video/avc"
        const val HEVC = "video/hevc"
        const val DOLBY_VISION = "video/dolby-vision"

        /**
         * Codec names as the server knows them, with the decoder to look for. HEVC
         * needs Main 10: most HEVC files are 10-bit.
         */
        val VIDEO = listOf(
            Triple("h264", H264, null),
            Triple("hevc", HEVC, CodecProfileLevel.HEVCProfileMain10),
            Triple("av1", "video/av01", null),
            Triple("vp9", "video/x-vnd.on2.vp9", null),
        )

        val AUDIO = listOf(
            "aac" to "audio/mp4a-latm",
            "mp3" to "audio/mpeg",
            "opus" to "audio/opus",
            "flac" to "audio/flac",
            "ac3" to "audio/ac3",
            "eac3" to "audio/eac3",
        )

        val PASSTHROUGH = listOf(
            "ac3" to C.ENCODING_AC3,
            "eac3" to C.ENCODING_E_AC3,
            "dts" to C.ENCODING_DTS,
            "truehd" to C.ENCODING_DOLBY_TRUEHD,
        )

        /** Media3 reads these itself, whatever the device's own decoders. */
        val CONTAINERS = listOf("mp4", "mkv", "webm", "mpegts")
    }
}

/**
 * Whether this decoder takes `mime`, with `profile` if given. Software decoders
 * don't count for the newer codecs: they'd be sent files they can't keep up with,
 * which looks like a stutter rather than a failure the fallback could catch.
 */
private fun MediaCodecInfo.decodes(mime: String, profile: Int? = null, needsHardware: Boolean = false): Boolean {
    val type = supportedTypes.firstOrNull { it.equals(mime, ignoreCase = true) } ?: return false
    if (needsHardware && Build.VERSION.SDK_INT >= 29 && !isHardwareAccelerated) {
        return false
    }
    if (profile == null) {
        return true
    }
    return runCatching { getCapabilitiesForType(type).profileLevels.any { it.profile == profile } }.getOrDefault(false)
}
