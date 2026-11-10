package app.mediacabinet.core.playback

import app.mediacabinet.core.api.Choice
import app.mediacabinet.core.api.PlayPlan

// Ports of web/src/core/subtitles.ts and tracks.ts, which are the reference.

/** A subtitle: its time on screen in seconds of the file, and its text without markup. */
data class Cue(val start: Double, val end: Double, val text: String)

private val TIMING = Regex("""(\d+:)?(\d+):(\d+)[.,](\d+)\s*-->\s*(\d+:)?(\d+):(\d+)[.,](\d+)""")

private fun seconds(hours: String, minutes: String, whole: String, fraction: String): Double {
    val hour = hours.trimEnd(':').toIntOrNull() ?: 0
    return hour * 3600 + minutes.toInt() * 60 + whole.toInt() + "0.$fraction".toDouble()
}

private fun plainText(lines: List<String>) = lines.joinToString("\n")
    .replace(Regex("<[^>]*>"), "")
    .replace("&nbsp;", " ")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&amp;", "&")
    .trim()

/** Reads WebVTT. Styling and positions are left out: cues are shown as plain text. */
fun parseVtt(vtt: String): List<Cue> {
    val cues = mutableListOf<Cue>()
    for (block in vtt.replace(Regex("\r\n?"), "\n").split(Regex("\n{2,}"))) {
        val lines = block.split("\n")
        for ((i, line) in lines.withIndex()) {
            val timing = TIMING.find(line) ?: continue
            val g = timing.groupValues
            val text = plainText(lines.drop(i + 1))
            if (text.isNotEmpty()) {
                cues += Cue(seconds(g[1], g[2], g[3], g[4]), seconds(g[5], g[6], g[7], g[8]), text)
            }
            break
        }
    }
    return cues
}

/** What's on screen at this time. */
fun List<Cue>.textAt(time: Double): String =
    filter { it.start <= time && time < it.end }.joinToString("\n") { it.text }

/**
 * Whether a change needs a new stream. Text subtitles are drawn over the video,
 * so only they can change without one; picture subtitles are burned in, both
 * when turning them on and when turning them off.
 */
fun needsNewStream(plan: PlayPlan, from: Choice, to: Choice): Boolean {
    val sameStream = to.fileId == from.fileId && to.audioStream == from.audioStream && to.maxHeight == from.maxHeight
    val track = plan.version.subtitles.firstOrNull { it.id == to.subtitle }
    val burning = plan.subtitles.any { it.url == null }
    return !sameStream || !(track?.text ?: true) || burning
}

/** Where the chosen subtitles are, when they're drawn over the video. */
fun subtitleUrl(plan: PlayPlan, choice: Choice): String? =
    plan.subtitles.firstOrNull { it.id == choice.subtitle }?.url
