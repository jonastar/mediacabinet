package app.mediacabinet.phone.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.mediacabinet.core.Tokens
import coil3.compose.AsyncImage
import kotlinx.coroutines.CancellationException
import okhttp3.HttpUrl

/** Something fetched for a screen. */
sealed interface Load<out T> {
    data object Loading : Load<Nothing>
    data class Failed(val message: String) : Load<Nothing>
    data class Ready<T>(val value: T) : Load<T>
}

/** Fetches again whenever a key changes, such as a retry count. */
@Composable
fun <T> rememberLoad(vararg keys: Any?, fetch: suspend () -> T): State<Load<T>> =
    produceState<Load<T>>(Load.Loading, *keys) {
        value = Load.Loading
        value = try {
            Load.Ready(fetch())
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Load.Failed(e.message ?: "The server couldn't be reached")
        }
    }

val CardShape = RoundedCornerShape(Tokens.Radius.card.dp)

/** A titled row that scrolls sideways. */
fun <T> LazyListScope.rail(title: String, items: List<T>, key: (T) -> Any, item: @Composable (T) -> Unit) {
    if (items.isEmpty()) {
        return
    }
    item(key = "rail-$title") {
        Column(Modifier.padding(top = Tokens.Space.space24.dp)) {
            Text(
                title,
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.padding(horizontal = Tokens.Space.space16.dp, vertical = Tokens.Space.space8.dp),
            )
            LazyRow(
                contentPadding = PaddingValues(horizontal = Tokens.Space.space16.dp),
                horizontalArrangement = Arrangement.spacedBy(Tokens.Space.space12.dp),
            ) {
                items(items.size, key = { key(items[it]) }) { item(items[it]) }
            }
        }
    }
}

/** Accent on a dim track, the same on every card and in the player. */
@Composable
fun ProgressBar(fraction: Float, modifier: Modifier = Modifier) {
    Box(
        modifier
            .fillMaxWidth()
            .height(4.dp)
            .clip(RoundedCornerShape(Tokens.Radius.pill.dp))
            .background(Palette.text.copy(alpha = 0.25f)),
    ) {
        Box(
            Modifier
                .fillMaxWidth(fraction.coerceIn(0f, 1f))
                .height(4.dp)
                .background(Palette.accent),
        )
    }
}

@Composable
fun Artwork(url: HttpUrl?, modifier: Modifier) {
    Box(modifier.clip(CardShape).background(Palette.surfaceRaised)) {
        if (url != null) {
            AsyncImage(
                model = url.toString(),
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

/** 2:3, title below it. */
@Composable
fun Poster(title: String, image: HttpUrl?, width: Dp = 120.dp, onClick: () -> Unit) {
    Column(Modifier.width(width).clickable(onClick = onClick)) {
        Artwork(image, Modifier.fillMaxWidth().aspectRatio(2f / 3f))
        Spacer(Modifier.height(Tokens.Space.space8.dp))
        Text(title, style = MaterialTheme.typography.labelMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/** 16:9, with a progress bar when started. For keep watching and episodes. */
@Composable
fun LandscapeCard(title: String, detail: String?, image: HttpUrl?, watched: Float, onClick: () -> Unit) {
    Column(Modifier.width(240.dp).clickable(onClick = onClick)) {
        Box {
            Artwork(image, Modifier.fillMaxWidth().aspectRatio(16f / 9f))
            if (watched > 0f) {
                ProgressBar(watched, Modifier.align(Alignment.BottomCenter).padding(Tokens.Space.space8.dp))
            }
        }
        Spacer(Modifier.height(Tokens.Space.space8.dp))
        Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (detail != null) {
            Text(
                detail,
                style = MaterialTheme.typography.bodyMedium,
                color = Palette.textSecondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
fun Loading() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        CircularProgressIndicator()
    }
}

/** A problem in words, with what can be done about it. */
@Composable
fun Problem(
    title: String,
    message: String,
    onRetry: () -> Unit,
    otherLabel: String? = null,
    onOther: () -> Unit = {},
) {
    Column(
        Modifier.fillMaxSize().padding(Tokens.Space.space24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(title, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
        Spacer(Modifier.height(Tokens.Space.space8.dp))
        Text(message, color = Palette.textSecondary, textAlign = TextAlign.Center)
        Spacer(Modifier.height(Tokens.Space.space24.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(Tokens.Space.space12.dp)) {
            Button(onClick = onRetry) {
                Text("Try again")
            }
            if (otherLabel != null) {
                OutlinedButton(onClick = onOther) {
                    Text(otherLabel)
                }
            }
        }
    }
}
