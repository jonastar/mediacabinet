package app.mediacabinet.core

import app.mediacabinet.core.api.EpisodeNumber
import app.mediacabinet.core.api.NumberStyle
import kotlin.math.min
import kotlin.math.roundToInt

// Ports of web/src/core/episode.ts and progress.ts, which are the reference.

private fun pad(n: Long) = n.toString().padStart(2, '0')

/** `S01E03`, `03` or `E03`, following the ordering the number comes from. */
fun episodeLabel(n: EpisodeNumber?): String {
    val number = n?.number ?: return "—"
    if (n.season != null && n.style != NumberStyle.Absolute) {
        return "S${pad(n.season)}E${pad(number)}"
    }
    return when (n.style) {
        NumberStyle.Playlist -> "E${pad(number)}"
        else -> pad(number)
    }
}

/** `S01E03 · Rabbot`, or just the title when there's no number to show. */
fun episodeLine(number: EpisodeNumber?, title: String): String = when (number?.number) {
    null -> title
    else -> "${episodeLabel(number)} · $title"
}

/** The season an episode is in, or null when its ordering has no seasons. */
fun seasonOf(n: EpisodeNumber?): Long? = when (n?.style) {
    NumberStyle.Season -> n.season
    else -> null
}

fun seasonName(season: Long): String = when (season) {
    0L -> "Specials"
    else -> "Season $season"
}

/** How far through it someone is, from 0 to 1: 1 once it's played, 0 until it's started. */
fun progress(runtime: Double?, position: Double, played: Boolean): Float {
    if (played) {
        return 1f
    }
    if (runtime == null || runtime <= 0 || position <= 0) {
        return 0f
    }
    return min(1.0, position / runtime).toFloat()
}

/** Whole minutes still to watch, at least 1; null when it isn't started or the length is unknown. */
fun minutesLeft(runtime: Double?, position: Double): Int? {
    if (runtime == null || runtime <= 0 || position <= 0) {
        return null
    }
    return maxOf(1, ((runtime - position) / 60).roundToInt())
}

/** `1:02:03` or `2:03`, for the player's timeline. */
fun clock(seconds: Double): String {
    val total = seconds.coerceAtLeast(0.0).toLong()
    val h = total / 3600
    val m = (total % 3600) / 60
    val s = total % 60
    return when {
        h > 0 -> "$h:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}"
        else -> "$m:${s.toString().padStart(2, '0')}"
    }
}

/**
 * Whether this app is at least `minimum` (`GET /api/server-info`), comparing
 * `major.minor.patch` and ignoring anything after a `-`.
 */
fun meetsMinimum(version: String, minimum: String): Boolean {
    val parse = { v: String -> v.substringBefore('-').split('.').map { it.toIntOrNull() ?: 0 } }
    val ours = parse(version)
    val theirs = parse(minimum)
    for (i in 0 until maxOf(ours.size, theirs.size)) {
        val a = ours.getOrElse(i) { 0 }
        val b = theirs.getOrElse(i) { 0 }
        if (a != b) {
            return a > b
        }
    }
    return true
}
