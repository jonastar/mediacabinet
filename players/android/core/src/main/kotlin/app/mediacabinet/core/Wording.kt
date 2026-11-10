package app.mediacabinet.core

import app.mediacabinet.core.api.Availability
import app.mediacabinet.core.api.Episode
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonPrimitive
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.roundToInt

// Watch mode wording from web/src/lib/wording.ts and format.ts, which are the
// source: a change there includes this file (docs/design-language.md).

/** Badge tones map to the state colours in the tokens. */
enum class Tone { Ready, Watched, Next, Missing, Upcoming, Neutral }

data class Badge(val label: String, val tone: Tone)

/** An external tool's live status, e.g. "Downloading 40%". Stale statuses are hidden. */
fun liveStatus(episode: Episode): String? {
    val status = episode.statuses.firstOrNull { !it.stale } ?: return null
    val name = status.status.replaceFirstChar { it.uppercase() }
    val progress = status.detail?.get("progress")?.jsonPrimitive?.doubleOrNull
    return when (progress) {
        null -> name
        else -> "$name ${(progress * 100).roundToInt()}%"
    }
}

/** What an episode's badge says to a viewer. */
fun episodeBadge(episode: Episode, isUpNext: Boolean): Badge {
    if (isUpNext) {
        return Badge("Up next", Tone.Next)
    }
    return when (episode.availability) {
        Availability.Available -> when (episode.watch?.played) {
            true -> Badge("Watched", Tone.Watched)
            else -> Badge("Ready", Tone.Ready)
        }
        Availability.Missing -> Badge("File offline", Tone.Missing)
        Availability.NotAcquired -> when (val status = liveStatus(episode)) {
            null -> Badge("Not in library", Tone.Neutral)
            else -> Badge(status, Tone.Upcoming)
        }
        Availability.Upcoming -> Badge("Airs ${airShortDay(episode.airsAt ?: 0, episode.airsTimeKnown)}", Tone.Upcoming)
        else -> Badge("Date TBA", Tone.Neutral)
    }
}

private val dayFormat = DateTimeFormatter.ofPattern("EEE d MMM", Locale.getDefault())

/**
 * Date-only air times are stored as the end of that day in UTC; step back into
 * the day itself, so time zones west of UTC don't show the next day.
 */
fun airDay(seconds: Long, timeKnown: Boolean): LocalDate = when {
    timeKnown -> Instant.ofEpochSecond(seconds).atZone(ZoneId.systemDefault()).toLocalDate()
    else -> Instant.ofEpochSecond(seconds - 12 * 3600).atOffset(ZoneOffset.UTC).toLocalDate()
}

/** `Fri 30 Oct`. */
fun airShortDay(seconds: Long, timeKnown: Boolean): String = dayFormat.format(airDay(seconds, timeKnown))
