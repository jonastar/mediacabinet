package app.mediacabinet.phone.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.mediacabinet.core.Tokens
import app.mediacabinet.core.api.Availability
import app.mediacabinet.core.api.Episode
import app.mediacabinet.core.api.EpisodeList
import app.mediacabinet.core.api.SeriesDetail
import app.mediacabinet.core.episodeBadge
import app.mediacabinet.core.episodeLabel
import app.mediacabinet.core.episodeLine
import app.mediacabinet.core.progress
import app.mediacabinet.core.seasonName
import app.mediacabinet.core.seasonOf
import app.mediacabinet.phone.AppModel
import app.mediacabinet.phone.Screen
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope

@Composable
fun HomeScreen(model: AppModel) {
    var attempt by remember { mutableIntStateOf(0) }
    val api = remember(model.account.server) { model.api() }
    val load by rememberLoad(attempt) { api.home() }

    when (val state = load) {
        Load.Loading -> Loading()
        is Load.Failed -> Problem(
            title = "Couldn't load your library",
            message = state.message,
            onRetry = { attempt++ },
            otherLabel = "Change server",
            onOther = model::changeServer,
        )
        is Load.Ready -> {
            val home = state.value
            val empty = home.keepWatching.isEmpty() && home.shows.isEmpty() && home.movies.isEmpty()
            LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = Tokens.Space.space32.dp)) {
                item(key = "top") {
                    TopBar(model)
                }
                if (empty) {
                    item(key = "empty") {
                        Text(
                            "There's nothing to watch here yet.",
                            color = Palette.textMuted,
                            modifier = Modifier.padding(Tokens.Space.space16.dp),
                        )
                    }
                }
                rail("Keep watching", home.keepWatching, key = { it.episode.itemId }) { item ->
                    // The episode's own picture, or else the show's.
                    val show = home.shows.firstOrNull { it.id == item.seriesId }
                    val image = api.artwork(item.episode.itemId, item.episode.artwork, "still")
                        ?: show?.let { api.artwork(it.id, it.artwork, "backdrop", "poster") }
                    LandscapeCard(
                        title = item.seriesTitle,
                        detail = episodeLine(item.episode.number, item.episode.title),
                        image = image,
                        watched = progress(item.episode.runtimeSeconds?.toDouble(), item.positionSeconds, false),
                        onClick = { model.open(Screen.Player(item.episode.itemId, item.seriesTitle)) },
                    )
                }
                rail("Shows", home.shows, key = { it.id }) { show ->
                    Poster(show.title, api.artwork(show.id, show.artwork, "poster")) {
                        model.open(Screen.Series(show.id, show.title))
                    }
                }
                rail("Movies", home.movies, key = { it.id }) { movie ->
                    Poster(movie.title, api.artwork(movie.id, movie.artwork, "poster")) {
                        model.open(Screen.Player(movie.id, movie.title))
                    }
                }
                item(key = "inset") {
                    Spacer(Modifier.windowInsetsBottomHeight(WindowInsets.navigationBars))
                }
            }
        }
    }
}

@Composable
private fun TopBar(model: AppModel) {
    var menu by remember { mutableStateOf(false) }
    Row(
        Modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .padding(start = Tokens.Space.space16.dp, top = Tokens.Space.space8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text("Home", style = MaterialTheme.typography.headlineMedium)
            model.serverInfo?.name?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = Palette.textMuted) }
        }
        Box {
            IconButton(onClick = { menu = true }) {
                Icon(Icons.Default.MoreVert, contentDescription = "More")
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(text = { Text("Sign out") }, onClick = {
                    menu = false
                    model.signOut()
                })
                DropdownMenuItem(text = { Text("Change server") }, onClick = {
                    menu = false
                    model.changeServer()
                })
                DropdownMenuItem(text = { Text("Forget playback problems") }, onClick = {
                    menu = false
                    model.caps.forget()
                })
            }
        }
    }
}

private data class SeriesPage(val series: SeriesDetail, val episodes: EpisodeList)

@Composable
fun SeriesScreen(model: AppModel, seriesId: Long, title: String) {
    var attempt by remember { mutableIntStateOf(0) }
    val api = remember(model.account.server) { model.api() }
    val load by rememberLoad(seriesId, attempt) {
        coroutineScope {
            val series = async { api.series(seriesId) }
            val episodes = async { api.episodes(seriesId) }
            SeriesPage(series.await(), episodes.await())
        }
    }

    Column(Modifier.fillMaxSize()) {
        Row(Modifier.statusBarsPadding(), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { model.back() }) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
            }
            Text(title, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        when (val state = load) {
            Load.Loading -> Loading()
            is Load.Failed -> Problem("Couldn't load this show", state.message, onRetry = { attempt++ })
            is Load.Ready -> {
                val (series, list) = state.value
                val seasons = list.entries.filterNot { it.isGap }.groupBy { seasonOf(it.number) }
                LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = Tokens.Space.space32.dp)) {
                    item(key = "header") {
                        Column(Modifier.padding(horizontal = Tokens.Space.space16.dp)) {
                            Artwork(
                                api.artwork(series.id, series.artwork, "backdrop", "poster"),
                                Modifier.fillMaxWidth().aspectRatio(16f / 9f),
                            )
                            Spacer(Modifier.height(Tokens.Space.space12.dp))
                            listOfNotNull(series.year?.toString(), series.genres.take(3).joinToString(", ").ifEmpty { null })
                                .joinToString(" · ")
                                .takeIf { it.isNotEmpty() }
                                ?.let { Text(it, color = Palette.textMuted, style = MaterialTheme.typography.bodyMedium) }
                            series.overview?.let {
                                Spacer(Modifier.height(Tokens.Space.space8.dp))
                                Text(it, color = Palette.textSecondary)
                            }
                        }
                    }
                    for ((season, episodes) in seasons) {
                        if (season != null) {
                            item(key = "season-$season") {
                                Text(
                                    seasonName(season),
                                    style = MaterialTheme.typography.titleLarge,
                                    modifier = Modifier.padding(
                                        start = Tokens.Space.space16.dp,
                                        top = Tokens.Space.space24.dp,
                                        bottom = Tokens.Space.space8.dp,
                                    ),
                                )
                            }
                        }
                        items(episodes.size, key = { episodes[it].itemId }) { i ->
                            val episode = episodes[i]
                            EpisodeRow(model, episode, upNext = episode.itemId == list.upNext, seriesTitle = series.title)
                        }
                    }
                    item(key = "inset") {
                        Spacer(Modifier.windowInsetsBottomHeight(WindowInsets.navigationBars))
                    }
                }
            }
        }
    }
}

@Composable
private fun EpisodeRow(model: AppModel, episode: Episode, upNext: Boolean, seriesTitle: String) {
    val api = remember(model.account.server) { model.api() }
    // Missing files can't play; the rest aren't there to play yet.
    val playable = episode.availability == Availability.Available
    val watch = episode.watch
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(enabled = playable) { model.open(Screen.Player(episode.itemId, seriesTitle)) }
            .padding(horizontal = Tokens.Space.space16.dp, vertical = Tokens.Space.space8.dp),
        horizontalArrangement = Arrangement.spacedBy(Tokens.Space.space12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(128.dp)) {
            Artwork(api.artwork(episode.itemId, episode.artwork, "still"), Modifier.fillMaxWidth().aspectRatio(16f / 9f))
            val watched = progress(episode.runtimeSeconds?.toDouble(), watch?.positionSeconds ?: 0.0, watch?.played ?: false)
            if (watched > 0f) {
                ProgressBar(watched, Modifier.align(Alignment.BottomCenter).padding(Tokens.Space.space4.dp))
            }
        }
        Column(Modifier.weight(1f)) {
            Text(
                episodeLabel(episode.number),
                style = MaterialTheme.typography.labelSmall,
                color = Palette.textMuted,
            )
            Text(
                episode.title,
                style = MaterialTheme.typography.titleMedium,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                color = when {
                    playable -> Palette.text
                    else -> Palette.textMuted
                },
            )
            val badge = episodeBadge(episode, upNext)
            Text(badge.label, style = MaterialTheme.typography.labelSmall, color = toneColor(badge.tone))
        }
    }
}
