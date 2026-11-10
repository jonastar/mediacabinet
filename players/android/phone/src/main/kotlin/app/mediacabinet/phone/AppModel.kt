package app.mediacabinet.phone

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.mediacabinet.core.api.Api
import app.mediacabinet.core.api.ServerInfo
import kotlinx.coroutines.launch

/** Where the app is. The last screen in the stack is the one showing. */
sealed interface Screen {
    data object Connect : Screen
    data object SignIn : Screen
    data object Home : Screen
    data class Series(val id: Long, val title: String) : Screen
    data class Player(val itemId: Long, val title: String) : Screen
}

/** The app's navigation and account state, kept across rotation. */
class AppModel(application: Application) : AndroidViewModel(application) {
    private val app = application as CabinetApp
    val account = app.account
    val caps = app.caps
    val http = app.http.client

    val stack = mutableStateListOf<Screen>(
        when {
            account.server == null -> Screen.Connect
            account.token == null -> Screen.SignIn
            else -> Screen.Home
        },
    )

    /** The server's name and rules, once fetched for the sign-in screen. */
    var serverInfo by mutableStateOf<ServerInfo?>(null)

    init {
        viewModelScope.launch {
            app.signedOut.collect { show(Screen.SignIn) }
        }
    }

    /** The API of the chosen server. Only called once there is one. */
    fun api(): Api = app.http.api(account.server ?: error("no server chosen"))

    val current: Screen
        get() = stack.last()

    fun open(screen: Screen) {
        stack.add(screen)
    }

    /** Starts again from this screen, as after signing in or out. */
    fun show(screen: Screen) {
        stack.clear()
        stack.add(screen)
    }

    /** Goes back a screen; false when there's nowhere to go back to. */
    fun back(): Boolean {
        if (stack.size <= 1) {
            return false
        }
        stack.removeAt(stack.lastIndex)
        return true
    }

    fun signedIn(token: String) {
        account.token = token
        show(Screen.Home)
    }

    fun signOut() {
        endSession()
        show(Screen.SignIn)
    }

    fun changeServer() {
        endSession()
        account.server = null
        serverInfo = null
        show(Screen.Connect)
    }

    /**
     * Ends the device's session on the server too, so it leaves Account → Devices.
     * The token is still needed for that request, so it's forgotten afterwards.
     */
    private fun endSession() {
        val token = account.token ?: return
        val api = api()
        viewModelScope.launch {
            runCatching { api.signOut(token) }
            if (account.token == token) {
                account.token = null
            }
        }
    }
}
