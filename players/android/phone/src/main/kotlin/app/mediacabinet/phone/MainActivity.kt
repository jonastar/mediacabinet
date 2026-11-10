package app.mediacabinet.phone

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.lifecycle.viewmodel.compose.viewModel
import app.mediacabinet.phone.ui.CabinetTheme
import app.mediacabinet.phone.ui.ConnectScreen
import app.mediacabinet.phone.ui.HomeScreen
import app.mediacabinet.phone.ui.PlayerScreen
import app.mediacabinet.phone.ui.SeriesScreen
import app.mediacabinet.phone.ui.SignInScreen

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContent {
            CabinetTheme {
                App(viewModel())
            }
        }
    }
}

@Composable
private fun App(model: AppModel) {
    BackHandler(enabled = model.stack.size > 1) {
        model.back()
    }
    Surface(color = MaterialTheme.colorScheme.background, modifier = Modifier.fillMaxSize()) {
        when (val screen = model.current) {
            Screen.Connect -> ConnectScreen(model)
            Screen.SignIn -> SignInScreen(model)
            Screen.Home -> HomeScreen(model)
            is Screen.Series -> SeriesScreen(model, screen.id, screen.title)
            is Screen.Player -> PlayerScreen(model, screen.itemId, screen.title)
        }
    }
}
