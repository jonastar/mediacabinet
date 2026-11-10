package app.mediacabinet.phone

import android.app.Application
import app.mediacabinet.core.Account
import app.mediacabinet.core.Http
import app.mediacabinet.core.playback.Caps
import coil3.ImageLoader
import coil3.PlatformContext
import coil3.SingletonImageLoader
import coil3.network.okhttp.OkHttpNetworkFetcherFactory
import coil3.request.crossfade
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow

/** What lasts as long as the app: the account, one HTTP client, and capabilities. */
class CabinetApp : Application(), SingletonImageLoader.Factory {
    lateinit var account: Account
        private set
    lateinit var http: Http
        private set
    lateinit var caps: Caps
        private set

    private val signedOutEvents = MutableSharedFlow<Unit>(extraBufferCapacity = 1)

    /** The server stopped accepting the token: signed out on the web, or expired. */
    val signedOut: SharedFlow<Unit> = signedOutEvents

    override fun onCreate() {
        super.onCreate()
        account = Account(this)
        http = Http(account) { signedOutEvents.tryEmit(Unit) }
        caps = Caps(this)
    }

    /** Images come through the same client, so artwork gets the device token. */
    override fun newImageLoader(context: PlatformContext): ImageLoader = ImageLoader.Builder(context)
        .components { add(OkHttpNetworkFetcherFactory(callFactory = { http.client })) }
        .crossfade(true)
        .build()
}
