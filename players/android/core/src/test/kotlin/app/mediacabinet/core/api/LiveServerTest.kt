package app.mediacabinet.core.api

import app.mediacabinet.core.playback.parseVtt
import kotlinx.coroutines.runBlocking
import okhttp3.HttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test

/**
 * Decodes a real server's responses with the generated models, and fetches the
 * start of a few streams the way Media3 does, with the bearer token. Skipped
 * unless pointed at a server, with a device token or a name and password:
 *
 *     CABINET_TEST_SERVER=http://127.0.0.1:8420 CABINET_TEST_TOKEN=… \
 *       ./gradlew :core:testDebugUnitTest --tests '*LiveServerTest'
 *
 * It starts and stops real streams, so a transcode briefly runs on the server.
 */
class LiveServerTest {
    private val address = System.getenv("CABINET_TEST_SERVER")
    private var token: String? = System.getenv("CABINET_TEST_TOKEN")
    private val client = OkHttpClient.Builder()
        .addInterceptor { chain ->
            val request = chain.request()
            val sent = token?.let { request.newBuilder().header("Authorization", "Bearer $it").build() }
            chain.proceed(sent ?: request)
        }
        .build()

    /** What a modern phone might report, and what forces the server to convert. */
    private val profiles = mapOf(
        "rich" to ClientCaps(
            containers = listOf("mp4", "mkv", "webm"),
            videoCodecs = listOf("h264", "hevc", "vp9"),
            audioCodecs = listOf("aac", "mp3", "opus", "flac", "ac3", "eac3"),
        ),
        "basic" to ClientCaps(containers = listOf("mp4"), videoCodecs = listOf("h264"), audioCodecs = listOf("aac")),
    )

    @Test
    fun `decodes what the app reads, and streams play with the token`() = runBlocking {
        assumeTrue("CABINET_TEST_SERVER not set", address != null)
        val api = Api(Api.normaliseAddress(address!!)!!, client)

        val info = api.serverInfo()
        println("server: ${info.name} ${info.version}, guest access ${info.guestAccess}")
        val signedInHere = token == null
        if (signedInHere) {
            token = api.signIn(System.getenv("CABINET_TEST_NAME")!!, System.getenv("CABINET_TEST_PASSWORD")!!, "LiveServerTest").token
        }
        try {
            val home = api.home()
            println("home: ${home.keepWatching.size} keep watching, ${home.shows.size} shows, ${home.movies.size} movies")
            val movies = api.movies()
            val shows = api.shows()
            var episodeCount = 0
            for (show in shows) {
                api.series(show.id)
                episodeCount += api.episodes(show.id).entries.size
            }
            println("decoded ${movies.size} movies, ${shows.size} shows, $episodeCount episodes")

            val items = (home.keepWatching.map { it.episode.itemId } +
                movies.filter { it.availability == Availability.Available }.map { it.id }).distinct().take(2)
            for (itemId in items) {
                for ((profile, caps) in profiles) {
                    stream(api, itemId, profile, caps)
                }
            }
        } finally {
            if (signedInHere) {
                api.signOut()
            }
        }
    }

    private suspend fun stream(api: Api, itemId: Long, profile: String, caps: ClientCaps) {
        val plan = try {
            api.play(itemId, Choice(), caps, null)
        } catch (e: ApiException) {
            println("play $itemId ($profile): ${e.status} ${e.message}")
            return
        }
        val v = plan.version
        println(
            "play $itemId ($profile): ${v.container} ${v.videoCodec} ${v.height}p ${v.hdrFormat ?: "SDR"} → " +
                "${plan.mode.value}, offset ${plan.offsetSeconds}, reasons ${plan.reasons}",
        )
        try {
            val url = api.url(plan.url)
            if (plan.mode == Mode.Direct) {
                val response = get(url, range = "bytes=0-1023")
                assertEquals("direct play answers a range request", 206, response.first)
                println("  direct: ${response.second.size} bytes")
            } else {
                val (status, body) = get(url)
                assertEquals(200, status)
                val playlist = String(body)
                assertTrue("an HLS playlist", playlist.startsWith("#EXTM3U"))
                // The init segment and the first media segment, as Media3 fetches them.
                val map = Regex("""#EXT-X-MAP:URI="([^"]+)"""").find(playlist)?.groupValues?.get(1)
                val first = playlist.lines().firstOrNull { it.isNotBlank() && !it.startsWith("#") }
                for (segment in listOfNotNull(map, first)) {
                    val (segmentStatus, bytes) = get(url.resolve(segment)!!)
                    assertEquals("segment $segment", 200, segmentStatus)
                    println("  $segment: ${bytes.size} bytes")
                }
                val type = Regex("""#EXT-X-PLAYLIST-TYPE:(\w+)""").find(playlist)?.groupValues?.get(1)
                println("  playlist type ${type ?: "none"}, ${playlist.lines().count { it.startsWith("#EXTINF") }} segments so far")
            }
            for (subtitle in plan.subtitles.filter { it.url != null }.take(1)) {
                val (status, body) = get(api.url(subtitle.url!!))
                println("  subtitles ${subtitle.id} (${subtitle.language}): $status, ${parseVtt(String(body)).size} cues")
            }
            // Without the token, the stream must be refused.
            val held = token
            token = null
            val (anonymous, _) = get(url)
            token = held
            assertEquals("a stream without the token", 401, anonymous)
        } finally {
            plan.sessionId?.let { api.stopStream(it) }
        }
    }

    private fun get(url: HttpUrl, range: String? = null): Pair<Int, ByteArray> {
        val request = Request.Builder().url(url).apply { range?.let { header("Range", it) } }.build()
        return client.newCall(request).execute().use { it.code to it.body.bytes() }
    }
}
