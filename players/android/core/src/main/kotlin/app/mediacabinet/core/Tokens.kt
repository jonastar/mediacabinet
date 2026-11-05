// Generated from design/tokens.json by scripts/generate-tokens.mjs. Do not edit.
// Colours are 0xAARRGGBB, as Compose's Color() takes them. TV sizes are at
// 1920×1080: halve them on Android TV, which lays out at 960×540 dp.

package app.mediacabinet.core

object Tokens {
    object Color {
        const val bg = 0xFF0F1115
        const val surface = 0xFF171A21
        const val surfaceRaised = 0xFF1F232C
        const val border = 0xFF262A33
        const val borderControl = 0xFF2E333D
        const val text = 0xFFE8E6E1
        const val textSecondary = 0xFFB5BAC3
        const val textMuted = 0xFF9AA0AA
        const val accent = 0xFF2BB5A6
        const val accentHover = 0xFF5FD0C3
        const val accentInk = 0xFF04201D
        const val accentSoftText = 0xFF8FDCD2
        const val stateAvailable = 0xFF4CC38A
        const val stateAvailableText = 0xFF6FD6A3
        const val stateMissing = 0xFFF07A5A
        const val stateMissingText = 0xFFF59C84
        const val stateUpcoming = 0xFF6EA8FE
        const val stateUpcomingText = 0xFF8DBBFF
        const val stateAttention = 0xFFF2B544
        const val stateAttentionText = 0xFFF5D08A
        const val stateNeutral = 0xFFB5BAC3
        const val stateNeutralBg = 0xFF22262E
        const val brandCabinet = 0xFF0F7A71
        const val brandPlay = 0xFF2BB5A6
        const val brandCream = 0xFFF2EEE6
    }

    object Radius {
        const val control = 10
        const val card = 14
        const val cardTv = 20
        const val pill = 999
    }

    object Space {
        const val space4 = 4
        const val space8 = 8
        const val space12 = 12
        const val space16 = 16
        const val space24 = 24
        const val space32 = 32
        const val space48 = 48
    }

    object PhoneType {
        const val hero = 34
        const val page = 28
        const val section = 20
        const val body = 15
        const val meta = 13
        const val small = 12
    }

    object TvType {
        const val hero = 76
        const val page = 57
        const val section = 38
        const val body = 29
        const val meta = 25
        const val small = 23
    }

    object DurationMs {
        const val focus = 150
    }

    object TvLayout {
        const val safeSide = 96
        const val safeTopBottom = 60
        const val posterWidth = 240
        const val posterHeight = 360
        const val cardWidth = 400
        const val cardHeight = 225
        const val focusRing = 4
        const val focusScale = 1.08
    }
}
