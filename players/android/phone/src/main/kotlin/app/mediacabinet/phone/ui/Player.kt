package app.mediacabinet.phone.ui

import android.app.Activity
import android.content.pm.ActivityInfo
import android.view.WindowManager
import androidx.annotation.OptIn
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.Lifecycle
import androidx.media3.common.util.UnstableApi
import androidx.media3.ui.compose.PlayerSurface
import androidx.media3.ui.compose.modifiers.resizeWithContentScale
import androidx.media3.ui.compose.state.rememberPresentationState
import app.mediacabinet.core.Tokens
import app.mediacabinet.core.clock
import app.mediacabinet.core.episodeLine
import app.mediacabinet.core.playback.PlaybackSession
import app.mediacabinet.core.playback.textAt
import app.mediacabinet.phone.AppModel
import kotlinx.coroutines.delay

private const val HIDE_CONTROLS_MS = 4000L
private const val SKIP_SECONDS = 10.0

@OptIn(UnstableApi::class)
@Composable
fun PlayerScreen(model: AppModel, itemId: Long, title: String) {
    val context = LocalContext.current
    val session = remember(itemId) {
        PlaybackSession(context.applicationContext, model.api(), model.http, itemId, model.caps).also { it.start() }
    }
    DisposableEffect(session) {
        onDispose { session.release() }
    }
    FullScreen()
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) {
        session.player.pause()
        session.report()
    }

    val state by session.state.collectAsStateWithLifecycle()
    var position by remember { mutableDoubleStateOf(0.0) }
    var scrubbing by remember { mutableStateOf<Double?>(null) }
    var controls by remember { mutableStateOf(true) }
    var lastTouch by remember { mutableStateOf(0L) }

    LaunchedEffect(session) {
        while (true) {
            position = session.position()
            delay(250)
        }
    }
    // Controls hide while playing, a moment after the last touch.
    LaunchedEffect(controls, state.playing, lastTouch, scrubbing) {
        if (controls && state.playing && scrubbing == null) {
            delay(HIDE_CONTROLS_MS)
            controls = false
        }
    }
    val touched = {
        lastTouch = System.currentTimeMillis()
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                controls = !controls
                touched()
            },
    ) {
        val presentation = rememberPresentationState(session.player)
        PlayerSurface(
            player = session.player,
            modifier = Modifier
                .align(Alignment.Center)
                .resizeWithContentScale(ContentScale.Fit, presentation.videoSizeDp),
        )

        val subtitle = state.cues.textAt(position)
        if (subtitle.isNotEmpty()) {
            Text(
                subtitle,
                color = Color.White,
                fontSize = 20.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .safeDrawingPadding()
                    .padding(
                        bottom = when {
                            controls -> 96.dp
                            else -> 24.dp
                        },
                        start = 24.dp,
                        end = 24.dp,
                    )
                    .background(Color.Black.copy(alpha = 0.6f), RoundedCornerShape(4.dp))
                    .padding(horizontal = 8.dp, vertical = 2.dp),
            )
        }

        val error = state.error
        when {
            error != null -> PlayerProblem(error, onRetry = session::retry, onClose = { model.back() })
            state.loading || state.plan == null -> CircularProgressIndicator(Modifier.align(Alignment.Center))
        }

        AnimatedVisibility(controls && error == null, enter = fadeIn(), exit = fadeOut(), modifier = Modifier.fillMaxSize()) {
            val plan = state.plan
            val heading = plan?.context?.let { c ->
                when (c.seriesTitle) {
                    null -> c.title
                    else -> episodeLine(c.number, c.title)
                }
            }
            Controls(
                title = plan?.context?.seriesTitle ?: plan?.context?.title ?: title,
                detail = heading.takeIf { plan?.context?.seriesTitle != null },
                playing = state.playing,
                position = scrubbing ?: position,
                duration = session.duration(),
                onBack = { model.back() },
                onToggle = {
                    session.togglePause()
                    touched()
                },
                onSkip = { seconds ->
                    session.seekBy(seconds)
                    touched()
                },
                onScrub = { scrubbing = it },
                onScrubEnd = {
                    scrubbing?.let(session::seekTo)
                    scrubbing = null
                    touched()
                },
            )
        }
    }
}

@Composable
private fun Controls(
    title: String,
    detail: String?,
    playing: Boolean,
    position: Double,
    duration: Double,
    onBack: () -> Unit,
    onToggle: () -> Unit,
    onSkip: (Double) -> Unit,
    onScrub: (Double) -> Unit,
    onScrubEnd: () -> Unit,
) {
    Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.45f)).safeDrawingPadding()) {
        Row(Modifier.align(Alignment.TopStart).padding(Tokens.Space.space8.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = Color.White)
            }
            Column {
                Text(title, style = MaterialTheme.typography.titleMedium, color = Color.White, maxLines = 1)
                detail?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = Palette.textSecondary, maxLines = 1) }
            }
        }
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .padding(horizontal = Tokens.Space.space16.dp, vertical = Tokens.Space.space8.dp),
        ) {
            // Left to right: play or pause, skip back and forward, then the timeline.
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Tokens.Space.space4.dp)) {
                IconButton(onClick = onToggle, modifier = Modifier.size(48.dp)) {
                    Icon(
                        when {
                            playing -> PauseIcon
                            else -> Icons.Default.PlayArrow
                        },
                        contentDescription = when {
                            playing -> "Pause"
                            else -> "Play"
                        },
                        tint = Color.White,
                    )
                }
                SkipButton("−10", "Back 10 seconds") { onSkip(-SKIP_SECONDS) }
                SkipButton("+10", "Forward 10 seconds") { onSkip(SKIP_SECONDS) }
                Text(
                    "${clock(position)} / ${clock(duration)}",
                    color = Color.White,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 13.sp,
                    modifier = Modifier.padding(start = Tokens.Space.space8.dp),
                )
            }
            Slider(
                value = position.toFloat(),
                onValueChange = { onScrub(it.toDouble()) },
                onValueChangeFinished = onScrubEnd,
                valueRange = 0f..duration.toFloat().coerceAtLeast(1f),
                colors = SliderDefaults.colors(
                    thumbColor = Palette.accent,
                    activeTrackColor = Palette.accent,
                    inactiveTrackColor = Color.White.copy(alpha = 0.25f),
                ),
            )
        }
    }
}

@Composable
private fun SkipButton(label: String, description: String, onClick: () -> Unit) {
    Box(
        Modifier
            .size(48.dp)
            .clickable(onClick = onClick, onClickLabel = description),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = Color.White, style = MaterialTheme.typography.labelLarge)
    }
}

@Composable
private fun PlayerProblem(message: String, onRetry: () -> Unit, onClose: () -> Unit) {
    Column(
        Modifier.fillMaxSize().safeDrawingPadding().padding(Tokens.Space.space24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(message, color = Color.White, textAlign = TextAlign.Center)
        Row(
            Modifier.padding(top = Tokens.Space.space24.dp),
            horizontalArrangement = Arrangement.spacedBy(Tokens.Space.space12.dp),
        ) {
            Button(onClick = onRetry) {
                Text("Try again")
            }
            OutlinedButton(onClick = onClose) {
                Text("Close")
            }
        }
    }
}

/**
 * Landscape, edge to edge without system bars (a swipe brings them back), and the
 * screen kept on, for as long as the player shows.
 */
@Composable
private fun FullScreen() {
    val view = LocalView.current
    DisposableEffect(view) {
        val activity = view.context as Activity
        val window = activity.window
        val insets = WindowCompat.getInsetsController(window, view)
        val orientation = activity.requestedOrientation
        activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        insets.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        insets.hide(WindowInsetsCompat.Type.systemBars())
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        onDispose {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            insets.show(WindowInsetsCompat.Type.systemBars())
            activity.requestedOrientation = orientation
        }
    }
}

/** Material's pause icon, which the core icon set leaves out. */
private val PauseIcon: ImageVector = ImageVector.Builder("Pause", 24.dp, 24.dp, 24f, 24f).apply {
    path(fill = SolidColor(Color.White)) {
        moveTo(6f, 19f)
        horizontalLineToRelative(4f)
        verticalLineTo(5f)
        horizontalLineTo(6f)
        close()
        moveTo(14f, 5f)
        verticalLineToRelative(14f)
        horizontalLineToRelative(4f)
        verticalLineTo(5f)
        close()
    }
}.build()
