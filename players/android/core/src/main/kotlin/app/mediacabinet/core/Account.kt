package app.mediacabinet.core

import android.content.Context
import android.os.Build
import android.provider.Settings
import app.mediacabinet.core.api.Api
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.Interceptor
import okhttp3.OkHttpClient
import java.util.concurrent.TimeUnit

/**
 * The server this app uses and its device token, kept in the app's private
 * storage. Signing out forgets the token and keeps the address.
 */
class Account(context: Context) {
    private val prefs = context.getSharedPreferences("account", Context.MODE_PRIVATE)

    var server: HttpUrl?
        get() = prefs.getString(SERVER, null)?.toHttpUrlOrNull()
        set(value) {
            prefs.edit().putString(SERVER, value?.toString()).apply()
        }

    var token: String?
        get() = prefs.getString(TOKEN, null)
        set(value) {
            prefs.edit().putString(TOKEN, value).apply()
        }

    /** What Account → Devices calls this device on the web. */
    val deviceName: String = Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)
        ?: "${Build.MANUFACTURER} ${Build.MODEL}"

    private companion object {
        const val SERVER = "server"
        const val TOKEN = "token"
    }
}

/**
 * One HTTP client for API calls, images and streams, which sends the device token
 * to the signed-in server, and only to it. A 401 from that server means the token
 * is gone (signed out on the web, or expired), and `onSignedOut` hears of it.
 */
class Http(private val account: Account, private val onSignedOut: () -> Unit) {
    val client: OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        // Starting a transcode can take a moment before the first byte.
        .readTimeout(30, TimeUnit.SECONDS)
        .addInterceptor(Interceptor { chain ->
            val request = chain.request()
            val server = account.server
            val token = account.token
            val ours = server != null && request.url.host == server.host && request.url.port == server.port
            val sent = if (ours && token != null && request.header("Authorization") == null) {
                request.newBuilder().header("Authorization", "Bearer $token").build()
            } else {
                request
            }
            val response = chain.proceed(sent)
            // Only for the token added here, and only if it's still the one in use.
            if (response.code == 401 && sent !== request && account.token == token) {
                account.token = null
                onSignedOut()
            }
            response
        })
        .build()

    fun api(server: HttpUrl): Api = Api(server, client)
}
