package app.mediacabinet.phone.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import app.mediacabinet.core.Tokens
import app.mediacabinet.core.api.Api
import app.mediacabinet.core.api.ApiException
import app.mediacabinet.core.api.PairingStarted
import app.mediacabinet.core.api.PairingState
import app.mediacabinet.core.meetsMinimum
import app.mediacabinet.phone.AppModel
import app.mediacabinet.phone.BuildConfig
import app.mediacabinet.phone.Screen
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
private fun Form(content: @Composable () -> Unit) {
    Column(
        Modifier
            .fillMaxSize()
            .safeDrawingPadding()
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(Tokens.Space.space24.dp),
        verticalArrangement = Arrangement.spacedBy(Tokens.Space.space16.dp),
    ) {
        Spacer(Modifier.height(Tokens.Space.space48.dp))
        content()
    }
}

/** A failure in words for the person, from whatever was thrown. */
private fun Throwable.explain(): String = when (this) {
    is ApiException -> message ?: "The server said no ($status)"
    else -> "Couldn't reach the server: ${message ?: javaClass.simpleName}"
}

@Composable
fun ConnectScreen(model: AppModel) {
    var address by remember { mutableStateOf(model.account.server?.toString()?.trimEnd('/') ?: "") }
    var problem by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    fun connect() {
        val server = Api.normaliseAddress(address)
        if (server == null) {
            problem = "That isn't a server address."
            return
        }
        busy = true
        problem = null
        scope.launch {
            try {
                val info = Api(server, model.http).serverInfo()
                if (!meetsMinimum(BuildConfig.VERSION_NAME, info.minAppVersion)) {
                    problem = "${info.name} needs version ${info.minAppVersion} of this app or later. Update it to connect."
                } else {
                    model.account.server = server
                    model.serverInfo = info
                    model.show(Screen.SignIn)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                problem = e.explain()
            } finally {
                busy = false
            }
        }
    }

    Form {
        Text("Connect to your server", style = MaterialTheme.typography.headlineMedium)
        Text(
            "The address you open Media Cabinet at in a browser, such as 192.168.1.20:8420 or cabinet.example.com.",
            color = Palette.textSecondary,
        )
        OutlinedTextField(
            value = address,
            onValueChange = { address = it },
            label = { Text("Server address") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
            keyboardActions = KeyboardActions(onGo = { connect() }),
            modifier = Modifier.fillMaxWidth(),
        )
        problem?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        Button(onClick = ::connect, enabled = !busy && address.isNotBlank(), modifier = Modifier.fillMaxWidth()) {
            Text(
                when {
                    busy -> "Connecting…"
                    else -> "Connect"
                },
            )
        }
    }
}

@Composable
fun SignInScreen(model: AppModel) {
    var name by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var problem by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var pairing by remember { mutableStateOf<PairingStarted?>(null) }
    val scope = rememberCoroutineScope()
    val uri = LocalUriHandler.current
    val api = remember(model.account.server) { model.api() }

    LaunchedEffect(api) {
        if (model.serverInfo == null) {
            model.serverInfo = runCatching { api.serverInfo() }.getOrNull()
        }
    }

    fun attempt(action: suspend () -> String) {
        busy = true
        problem = null
        scope.launch {
            try {
                model.signedIn(action())
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                problem = e.explain()
            } finally {
                busy = false
            }
        }
    }

    // Waits for the person to approve this phone in the browser.
    LaunchedEffect(pairing) {
        val started = pairing ?: return@LaunchedEffect
        while (true) {
            delay(started.pollEverySeconds * 1000)
            val status = try {
                api.pollPairing(started.secret)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                continue
            }
            val session = status.session
            when {
                session != null -> {
                    model.signedIn(session.token)
                    return@LaunchedEffect
                }
                status.state == PairingState.Declined -> {
                    problem = "Signing in was declined in the browser."
                    pairing = null
                }
                status.state == PairingState.Expired -> {
                    problem = "That took too long. Try again."
                    pairing = null
                }
            }
        }
    }

    val info = model.serverInfo
    Form {
        Text("Sign in", style = MaterialTheme.typography.headlineMedium)
        Text(info?.name ?: model.account.server.toString(), color = Palette.textSecondary)
        OutlinedTextField(
            value = name,
            onValueChange = { name = it },
            label = { Text("Name") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = password,
            onValueChange = { password = it },
            label = { Text("Password") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Go),
            keyboardActions = KeyboardActions(onGo = {
                attempt { api.signIn(name, password, model.account.deviceName).token }
            }),
            modifier = Modifier.fillMaxWidth(),
        )
        problem?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        Button(
            onClick = { attempt { api.signIn(name, password, model.account.deviceName).token } },
            enabled = !busy && name.isNotBlank() && password.isNotEmpty(),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Sign in")
        }
        HorizontalDivider(Modifier.padding(vertical = Tokens.Space.space8.dp))
        val started = pairing
        if (started == null) {
            OutlinedButton(
                onClick = {
                    problem = null
                    scope.launch {
                        try {
                            val next = api.startPairing(model.account.deviceName)
                            pairing = next
                            uri.openUri(api.url(next.path).toString())
                        } catch (e: CancellationException) {
                            throw e
                        } catch (e: Exception) {
                            problem = e.explain()
                        }
                    }
                },
                enabled = !busy,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Sign in with a browser")
            }
            Text(
                "For accounts that sign in with another service, or to avoid typing a password here.",
                style = MaterialTheme.typography.bodyMedium,
                color = Palette.textMuted,
            )
        } else {
            Text("Approve this phone in the browser. Code: ${started.code}", style = MaterialTheme.typography.titleMedium)
            Text("Waiting for approval…", color = Palette.textSecondary)
            TextButton(onClick = { pairing = null }) {
                Text("Cancel")
            }
        }
        if (info?.guestAccess == true) {
            OutlinedButton(
                onClick = { attempt { api.guest(model.account.deviceName).token } },
                enabled = !busy,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Continue as guest")
            }
        }
        TextButton(onClick = model::changeServer) {
            Text("Use another server")
        }
    }
}
