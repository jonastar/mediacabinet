package app.mediacabinet.core.api

import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import org.junit.Assume.assumeTrue
import org.junit.Test

/**
 * Decodes a real server's responses with the generated models. Skipped unless
 * pointed at one, such as a demo server (`seed-demo`):
 *
 *     CABINET_TEST_SERVER=http://127.0.0.1:8420 CABINET_TEST_NAME=… CABINET_TEST_PASSWORD=… \
 *       ./gradlew :core:testDebugUnitTest --tests '*LiveServerTest'
 */
class LiveServerTest {
    private val address = System.getenv("CABINET_TEST_SERVER")

    @Test
    fun `decodes what the app reads`() = runBlocking {
        assumeTrue("CABINET_TEST_SERVER not set", address != null)
        val server = Api.normaliseAddress(address!!)!!
        var token: String? = null
        val client = OkHttpClient.Builder()
            .addInterceptor { chain ->
                val request = chain.request()
                chain.proceed(token?.let { request.newBuilder().header("Authorization", "Bearer $it").build() } ?: request)
            }
            .build()
        val api = Api(server, client)

        val info = api.serverInfo()
        println("server: ${info.name} ${info.version}")
        token = api.signIn(System.getenv("CABINET_TEST_NAME"), System.getenv("CABINET_TEST_PASSWORD"), "LiveServerTest").token
        try {
            val home = api.home()
            println("home: ${home.keepWatching.size} keep watching, ${home.shows.size} shows, ${home.movies.size} movies")
            val shows = api.shows()
            val movies = api.movies()
            for (show in shows) {
                val series = api.series(show.id)
                val episodes = api.episodes(show.id)
                println("${series.title}: ${episodes.entries.size} episodes, up next ${episodes.upNext}")
            }
            println("${movies.size} movies")
            val caps = ClientCaps(containers = listOf("mp4"), videoCodecs = listOf("h264"), audioCodecs = listOf("aac"))
            val playable = movies.filter { it.availability == Availability.Available }.map { it.id } +
                home.keepWatching.map { it.episode.itemId }
            for (itemId in playable.take(3)) {
                try {
                    val plan = api.play(itemId, Choice(), caps, null)
                    println("play $itemId: ${plan.mode.value} ${plan.url} reasons=${plan.reasons}")
                    plan.sessionId?.let { api.stopStream(it) }
                } catch (e: ApiException) {
                    // Demo data has no real files; the error still has to decode.
                    println("play $itemId: ${e.status} ${e.message}")
                }
            }
        } finally {
            api.signOut()
        }
    }
}
