package app.mediacabinet.phone.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import app.mediacabinet.core.R
import app.mediacabinet.core.Tokens
import app.mediacabinet.core.Tone

/** Colours from `design/tokens.json`, as Compose takes them. */
object Palette {
    val bg = Color(Tokens.Color.bg)
    val surface = Color(Tokens.Color.surface)
    val surfaceRaised = Color(Tokens.Color.surfaceRaised)
    val border = Color(Tokens.Color.border)
    val text = Color(Tokens.Color.text)
    val textSecondary = Color(Tokens.Color.textSecondary)
    val textMuted = Color(Tokens.Color.textMuted)
    val accent = Color(Tokens.Color.accent)
    val accentInk = Color(Tokens.Color.accentInk)
    val missing = Color(Tokens.Color.stateMissingText)
    val available = Color(Tokens.Color.stateAvailableText)
    val upcoming = Color(Tokens.Color.stateUpcomingText)
    val neutral = Color(Tokens.Color.stateNeutral)
}

/** A badge's colour, from the state colours. */
fun toneColor(tone: Tone): Color = when (tone) {
    Tone.Ready -> Palette.available
    Tone.Next -> Palette.accent
    Tone.Missing -> Palette.missing
    Tone.Upcoming -> Palette.upcoming
    Tone.Watched, Tone.Neutral -> Palette.neutral
}

private fun manrope(weight: Int) = Font(
    R.font.manrope,
    weight = FontWeight(weight),
    variationSettings = FontVariation.Settings(FontVariation.weight(weight)),
)

val Manrope = FontFamily(manrope(400), manrope(500), manrope(600), manrope(700), manrope(800))

private fun style(size: Int, weight: Int, tight: Boolean = false) = TextStyle(
    fontFamily = Manrope,
    fontSize = size.sp,
    fontWeight = FontWeight(weight),
    letterSpacing = when {
        tight -> (-0.02).em
        else -> 0.em
    },
)

/** Type sizes from the tokens' phone column; titles are 800 with tight tracking. */
private val typography = Typography(
    displaySmall = style(Tokens.PhoneType.hero, 800, tight = true),
    headlineMedium = style(Tokens.PhoneType.page, 800, tight = true),
    titleLarge = style(Tokens.PhoneType.section, 800, tight = true),
    titleMedium = style(Tokens.PhoneType.body, 700),
    bodyLarge = style(Tokens.PhoneType.body, 400),
    bodyMedium = style(Tokens.PhoneType.meta, 400),
    labelLarge = style(Tokens.PhoneType.body, 700),
    labelMedium = style(Tokens.PhoneType.meta, 600),
    labelSmall = style(Tokens.PhoneType.small, 600),
)

private val colors = darkColorScheme(
    primary = Palette.accent,
    onPrimary = Palette.accentInk,
    background = Palette.bg,
    onBackground = Palette.text,
    surface = Palette.bg,
    onSurface = Palette.text,
    surfaceVariant = Palette.surface,
    onSurfaceVariant = Palette.textSecondary,
    surfaceContainer = Palette.surface,
    surfaceContainerHigh = Palette.surfaceRaised,
    outline = Palette.border,
    error = Palette.missing,
)

@Composable
fun CabinetTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = colors, typography = typography, content = content)
}
