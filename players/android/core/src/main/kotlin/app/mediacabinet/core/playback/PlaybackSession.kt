package app.mediacabinet.core.playback

import android.content.Context
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Tracks
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import app.mediacabinet.core.api.Api
import app.mediacabinet.core.api.Choice
import app.mediacabinet.core.api.ClientCaps
import app.mediacabinet.core.api.Mode
import app.mediacabinet.core.api.PlayPlan
import app.mediacabinet.core.api.await
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request

/** What a player screen draws from. Times are the file's, in seconds. */
data class PlaybackState(
    val plan: PlayPlan? = null,
    /** The version, quality, audio and subtitles in use. */
    val choice: Choice = Choice(),
    /** A request is out, or a seek is waiting to become one. */
    val loading: Boolean = true,
    val error: String? = null,
    val playing: Boolean = false,
    /** Ruled out while playing this item, for the stats panel. */
    val ruledOut: List<Failure> = emptyList(),
    val cues: List<Cue> = emptyList(),
)

/**
 * Playing one item, as `web/src/core/session.ts` does in browsers, which is the
 * reference: asks the server how, plays the plan in Media3, recovers from
 * failures, seeks past what's converted, and reports progress. It has no UI, so
 * the phone and TV apps draw their own controls from `state` and `position()`.
 *
 * The stream's times start at the plan's `offset_seconds` into the file, since an
 * HLS session can start partway in. Must be used from the main thread.
 */
@OptIn(UnstableApi::class)
class PlaybackSession(
    context: Context,
    private val api: Api,
    private val http: OkHttpClient,
    private val itemId: Long,
    private val caps: Caps,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    /** For the last report and stop, which outlive the session. */
    private val leaving = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    val player: ExoPlayer = ExoPlayer.Builder(context)
        .setMediaSourceFactory(DefaultMediaSourceFactory(context).setDataSourceFactory(OkHttpDataSource.Factory(http)))
        .setAudioAttributes(
            AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MOVIE).build(),
            true,
        )
        .setHandleAudioBecomingNoisy(true)
        .build()

    private val mutableState = MutableStateFlow(PlaybackState())
    val state: StateFlow<PlaybackState> = mutableState.asStateFlow()

    private var asking: ClientCaps = caps.forPlayback()
    private var unproven = mutableListOf<Failure>()
    private var started = false
    private var failedThisStream = false
    private var retries = 0
    private var lastRetryAt = 0L
    private var request: Job? = null
    private var seekTarget: Double? = null
    private var seekJob: Job? = null
    private var watchJob: Job? = null
    private var reportJob: Job? = null
    private var subtitleJob: Job? = null
    private var lastPosition = 0.0

    private val listener = object : Player.Listener {
        override fun onPlayerError(error: PlaybackException) {
            // 1xxx are general (timeouts, behind the live window) and 2xxx are
            // input and output: worth asking again. The rest are about the media.
            val network = error.errorCode in 1000..2999
            onFatal(error.errorCodeName.removePrefix("ERROR_CODE_").lowercase().replace('_', ' '), network)
        }

        override fun onTracksChanged(tracks: Tracks) {
            // Media3 plays what it can and drops the rest: a stream it can't
            // decode the picture or sound of plays on silently without it.
            for (type in listOf(C.TRACK_TYPE_VIDEO, C.TRACK_TYPE_AUDIO)) {
                if (tracks.containsType(type) && !tracks.isTypeSupported(type, false)) {
                    val what = when (type) {
                        C.TRACK_TYPE_VIDEO -> "picture"
                        else -> "sound"
                    }
                    onDecodeFailed("this device can't play the $what")
                    return
                }
            }
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            mutableState.update { it.copy(playing = isPlaying) }
        }
    }

    init {
        player.addListener(listener)
    }

    /** Starts playing, at `at` seconds or from where the viewer left off. */
    fun start(at: Double? = null) {
        reportJob = scope.launch {
            while (true) {
                delay(REPORT_EVERY_MS)
                report()
            }
        }
        load(at)
    }

    /** Where playback is in the file; where it's headed while a seek is pending. */
    fun position(): Double {
        seekTarget?.let { return it }
        val plan = state.value.plan ?: return 0.0
        if (player.playbackState == Player.STATE_IDLE) {
            return lastPosition
        }
        lastPosition = player.currentPosition / 1000.0 + plan.offsetSeconds
        return lastPosition
    }

    fun duration(): Double = state.value.plan?.version?.durationSeconds ?: 0.0

    /**
     * The subtitles on screen now. The server shifts cues by the plan's offset,
     * so they're matched against the stream's time, not the file's.
     */
    fun subtitleText(): String {
        if (seekTarget != null || player.playbackState == Player.STATE_IDLE) {
            return ""
        }
        return state.value.cues.textAt(player.currentPosition / 1000.0)
    }

    fun togglePause() {
        player.playWhenReady = !player.playWhenReady
    }

    /** Moves by this many seconds, in steps that add up while buttons are pressed. */
    fun seekBy(seconds: Double) {
        seekTo(position() + seconds)
    }

    fun seekTo(target: Double) {
        val plan = state.value.plan ?: return
        val end = duration().takeIf { it > 0 } ?: target
        val to = target.coerceIn(0.0, end)
        val inStream = to - plan.offsetSeconds
        if (plan.mode == Mode.Direct) {
            player.seekTo((to * 1000).toLong())
        } else if (inStream >= 0 && inStream <= convertedUntil()) {
            cancelPendingSeek()
            player.seekTo((inStream * 1000).toLong())
        } else {
            // Past what's converted: a new stream starts there, once the presses stop.
            seekTarget = to
            mutableState.update { it.copy(loading = true) }
            seekJob?.cancel()
            seekJob = scope.launch {
                delay(SEEK_SETTLE_MS)
                load(to)
            }
        }
    }

    /** Changes the version, quality, audio or subtitles, restarting only if it has to. */
    fun choose(next: Choice) {
        val current = state.value
        val restart = current.plan == null || needsNewStream(current.plan, current.choice, next)
        mutableState.update { it.copy(choice = next) }
        if (restart) {
            load(position())
        } else {
            loadSubtitles()
        }
    }

    /** Tries again after an error, from what's remembered. */
    fun retry() {
        asking = caps.forPlayback()
        unproven.clear()
        retries = 0
        lastRetryAt = 0
        load(position())
    }

    /** Reports the position and ends the stream, when leaving. */
    fun release() {
        report()
        stopServerSide(state.value.plan)
        scope.cancel()
        player.removeListener(listener)
        player.release()
    }

    fun report() {
        if (state.value.plan == null) {
            return
        }
        val position = position()
        val duration = duration().takeIf { it > 0 }
        leaving.launch {
            runCatching { api.progress(itemId, position, duration) }
        }
    }

    /** How far the stream is converted, in the stream's own time. */
    private fun convertedUntil(): Double {
        val duration = player.duration
        return if (duration == C.TIME_UNSET) {
            0.0
        } else {
            duration / 1000.0
        }
    }

    private fun cancelPendingSeek() {
        seekJob?.cancel()
        seekTarget = null
    }

    private fun stopServerSide(plan: PlayPlan?) {
        val session = plan?.sessionId ?: return
        leaving.launch {
            runCatching { api.stopStream(session) }
        }
    }

    private fun load(at: Double?) {
        request?.cancel()
        mutableState.update { it.copy(loading = true) }
        request = scope.launch {
            val plan = try {
                api.play(itemId, state.value.choice, asking, at)
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                seekTarget = null
                mutableState.update { it.copy(loading = false, error = e.message ?: "The server couldn't be reached") }
                return@launch
            }
            attach(plan)
        }
    }

    private fun attach(plan: PlayPlan) {
        stopServerSide(state.value.plan)
        watchJob?.cancel()
        val hls = plan.url.substringBefore('?').endsWith(".m3u8")
        val item = MediaItem.Builder().setUri(api.url(plan.url).toString())
        if (hls) {
            // The playlist grows as ffmpeg works, so Media3 sees a live stream.
            // Play it from the start at normal speed, not near its live edge.
            item.setMimeType(MimeTypes.APPLICATION_M3U8)
            item.setLiveConfiguration(
                MediaItem.LiveConfiguration.Builder().setMinPlaybackSpeed(1f).setMaxPlaybackSpeed(1f).build(),
            )
        }
        // Direct play starts at the resume point; an HLS stream starts there already.
        val startMs = if (plan.mode == Mode.Direct) {
            (plan.startSeconds * 1000).toLong()
        } else {
            0L
        }
        lastPosition = plan.startSeconds
        seekTarget = null
        started = false
        failedThisStream = false
        mutableState.update {
            it.copy(
                plan = plan,
                choice = it.choice.copy(fileId = plan.version.fileId, audioStream = plan.audioStream),
                loading = false,
                error = null,
            )
        }
        player.setMediaItem(item.build(), startMs)
        player.prepare()
        player.playWhenReady = true
        watchJob = scope.launch { watchForPicture() }
        loadSubtitles()
    }

    /**
     * Tells whether the stream starts playing, as `web/src/core/watchdog.ts`
     * does: started once it has played a moment, failed if it has data and
     * shows nothing for a while. A slow network isn't a failure.
     */
    private suspend fun watchForPicture() {
        var stuck = 0
        var played = 0L
        var last = player.currentPosition
        while (true) {
            delay(1000)
            val now = player.currentPosition
            if (player.isPlaying && now > last) {
                played += now - last
            }
            last = now
            if (played > PLAYED_MS) {
                onStarted()
                return
            }
            val buffered = player.bufferedPosition - now
            val waiting = player.playWhenReady && !player.isPlaying && buffered >= BUFFERED_ENOUGH_MS
            if (waiting) {
                stuck += 1
            } else {
                stuck = 0
            }
            if (stuck >= NO_PICTURE_SECONDS) {
                onDecodeFailed("no picture after $NO_PICTURE_SECONDS seconds")
                return
            }
        }
    }

    private fun onStarted() {
        started = true
        caps.remember(unproven)
        unproven.clear()
    }

    private fun onFatal(message: String, network: Boolean) {
        if (!network) {
            onDecodeFailed(message)
            return
        }
        // Retries are forgiven by a quiet spell, not by starting to play: a stream
        // that plays briefly and breaks each time would otherwise restart forever.
        val now = System.currentTimeMillis()
        if (now - lastRetryAt > RETRIES_FORGIVEN_AFTER_MS) {
            retries = 0
        }
        if (retries < NETWORK_RETRIES) {
            // A stream that ended on the server (idle too long) restarts where it was.
            retries += 1
            lastRetryAt = now
            load(position())
        } else {
            giveUp(message)
        }
    }

    private fun onDecodeFailed(message: String) {
        // One stream can fail several ways at once; the first one counts.
        if (failedThisStream) {
            return
        }
        failedThisStream = true
        watchJob?.cancel()
        val plan = state.value.plan
        val failure = plan?.let { nextFailure(it, asking) }
        if (failure == null) {
            giveUp(message)
            return
        }
        asking = asking.without(failure)
        // Something that broke partway through a file says little about the device.
        if (!started) {
            unproven += failure
        }
        mutableState.update { it.copy(ruledOut = it.ruledOut + failure) }
        load(position())
    }

    private fun giveUp(message: String) {
        player.stop()
        mutableState.update { it.copy(error = "Playback stopped: $message", loading = false) }
    }

    private fun loadSubtitles() {
        subtitleJob?.cancel()
        val current = state.value
        val url = current.plan?.let { subtitleUrl(it, current.choice) }
        if (url == null) {
            mutableState.update { it.copy(cues = emptyList()) }
            return
        }
        subtitleJob = scope.launch {
            val cues = runCatching {
                http.newCall(Request.Builder().url(api.url(url)).build()).await().use { parseVtt(it.body.string()) }
            }.getOrDefault(emptyList())
            mutableState.update { it.copy(cues = cues) }
        }
    }

    private companion object {
        const val REPORT_EVERY_MS = 15_000L
        /** Waits for more presses, so seeking doesn't start a stream per press. */
        const val SEEK_SETTLE_MS = 600L
        const val NETWORK_RETRIES = 2
        const val RETRIES_FORGIVEN_AFTER_MS = 5 * 60 * 1000L
        /** How long video may have data to play and still show nothing. */
        const val NO_PICTURE_SECONDS = 8
        /** Enough buffered that not playing can't be the network's fault. */
        const val BUFFERED_ENOUGH_MS = 2000L
        /** Played this much, so it's really playing, and not just a first frame. */
        const val PLAYED_MS = 200L
    }
}
