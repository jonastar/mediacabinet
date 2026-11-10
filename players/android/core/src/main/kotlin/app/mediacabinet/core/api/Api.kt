package app.mediacabinet.core.api

import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.serializer
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.io.IOException
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** A request the server turned down, with its `error` message when it gave one. */
class ApiException(val status: Int, message: String) : IOException(message)

/** The JSON settings every call uses: responses may have fields this app doesn't know yet. */
val ApiJson = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
}

/**
 * The calls this app makes, typed with the generated models. Like
 * `players/web/src/api.ts`, it lists only what the app uses; add calls here as
 * screens need them. The bearer token is added by the client (`Http.client`), so
 * images and streams get it too.
 */
class Api(val server: HttpUrl, private val http: OkHttpClient) {
    suspend fun serverInfo(): ServerInfo = get("/api/server-info")

    suspend fun signIn(name: String, password: String, deviceName: String): DeviceSession =
        post("/api/auth/device", DeviceCredentials(deviceName = deviceName, name = name, password = password))

    suspend fun guest(deviceName: String): DeviceSession =
        post("/api/auth/device/guest", GuestDevice(deviceName = deviceName))

    suspend fun startPairing(deviceName: String): PairingStarted =
        post("/api/auth/pair", PairingRequest(deviceName = deviceName))

    suspend fun pollPairing(secret: String): PairingStatus =
        post("/api/auth/pair/poll", PairingPoll(secret = secret))

    /** Ends a device session; with `token`, that one, whatever the account holds by now. */
    suspend fun signOut(token: String? = null) {
        val headers = token?.let { mapOf("Authorization" to "Bearer $it") } ?: emptyMap()
        send<Unit, Unit>("POST", "/api/auth/logout", null, headers)
    }

    suspend fun home(): Home = get("/api/home")

    suspend fun movies(): List<MovieSummary> = get("/api/movies")

    suspend fun shows(): List<SeriesSummary> = get("/api/series")

    suspend fun series(id: Long): SeriesDetail = get("/api/series/$id")

    /** In the viewer's chosen ordering or playlist for the series. */
    suspend fun episodes(seriesId: Long): EpisodeList = get("/api/series/$seriesId/episodes")

    suspend fun play(itemId: Long, choice: Choice, caps: ClientCaps, startSeconds: Double?): PlayPlan =
        post(
            "/api/items/$itemId/play",
            PlayRequest(
                audioStream = choice.audioStream,
                fileId = choice.fileId,
                maxHeight = choice.maxHeight,
                subtitle = choice.subtitle,
                caps = caps,
                startSeconds = startSeconds,
            ),
        )

    suspend fun stopStream(sessionId: String) {
        send<Unit, Unit>("DELETE", "/api/play/$sessionId", null)
    }

    suspend fun progress(itemId: Long, position: Double, duration: Double?) {
        send<Progress, Unit>("PUT", "/api/items/$itemId/progress", Progress(positionSeconds = position, durationSeconds = duration))
    }

    /** A server path such as a plan's `url` or an artwork path, as a full URL. */
    fun url(path: String): HttpUrl = server.resolve(path) ?: throw IllegalArgumentException("not a URL: $path")

    /** Artwork for an item, picking the first of `wanted` it has. */
    fun artwork(itemId: Long, kinds: List<String>, vararg wanted: String): HttpUrl? =
        wanted.firstOrNull { it in kinds }?.let { url("/api/artwork/$itemId/$it") }

    private suspend inline fun <reified T> get(path: String): T = send<Unit, T>("GET", path, null)

    private suspend inline fun <reified B, reified T> post(path: String, body: B): T = send<B, T>("POST", path, body)

    private suspend inline fun <reified B, reified T> send(
        method: String,
        path: String,
        body: B?,
        headers: Map<String, String> = emptyMap(),
    ): T = send(method, path, body, headers, serializer<B>(), serializer<T>())

    private suspend fun <B, T> send(
        method: String,
        path: String,
        body: B?,
        headers: Map<String, String>,
        bodySerializer: KSerializer<B>,
        resultSerializer: KSerializer<T>,
    ): T {
        val requestBody = body?.let { ApiJson.encodeToString(bodySerializer, it).toRequestBody(JSON) }
            ?: if (method == "GET" || method == "DELETE") {
                null
            } else {
                ByteArray(0).toRequestBody(null)
            }
        val request = Request.Builder().url(url(path)).method(method, requestBody)
            .apply { headers.forEach { (name, value) -> header(name, value) } }
            .build()
        http.newCall(request).await().use { response ->
            val text = response.body.string()
            if (!response.isSuccessful) {
                val message = runCatching { ApiJson.decodeFromString<ErrorBody>(text).error }.getOrNull()
                throw ApiException(response.code, message ?: "$method $path: ${response.code}")
            }
            if (resultSerializer.descriptor == Unit.serializer().descriptor) {
                @Suppress("UNCHECKED_CAST")
                return Unit as T
            }
            return ApiJson.decodeFromString(resultSerializer, text)
        }
    }

    companion object {
        private val JSON = "application/json".toMediaType()

        /** `tv.local:8420` as `http://tv.local:8420/`, or null if it can't be an address. */
        fun normaliseAddress(typed: String): HttpUrl? {
            val trimmed = typed.trim().trimEnd('/')
            if (trimmed.isEmpty()) {
                return null
            }
            val withScheme = if (Regex("^https?://", RegexOption.IGNORE_CASE).containsMatchIn(trimmed)) {
                trimmed
            } else {
                "http://$trimmed"
            }
            return "$withScheme/".toHttpUrlOrNull()
        }
    }
}

/** Runs a call without blocking, and cancels it with the coroutine. */
suspend fun Call.await(): Response = suspendCancellableCoroutine { continuation ->
    continuation.invokeOnCancellation { cancel() }
    enqueue(
        object : Callback {
            override fun onResponse(call: Call, response: Response) {
                continuation.resume(response)
            }

            override fun onFailure(call: Call, e: IOException) {
                continuation.resumeWithException(e)
            }
        },
    )
}
