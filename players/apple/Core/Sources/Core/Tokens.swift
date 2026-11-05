// Generated from design/tokens.json by scripts/generate-tokens.mjs. Do not edit.
// Colours are 0xRRGGBB. TV sizes are at 1920×1080, one-to-one on tvOS.

public enum Tokens {
    public enum Color {
        public static let bg: UInt32 = 0x0F1115
        public static let surface: UInt32 = 0x171A21
        public static let surfaceRaised: UInt32 = 0x1F232C
        public static let border: UInt32 = 0x262A33
        public static let borderControl: UInt32 = 0x2E333D
        public static let text: UInt32 = 0xE8E6E1
        public static let textSecondary: UInt32 = 0xB5BAC3
        public static let textMuted: UInt32 = 0x9AA0AA
        public static let accent: UInt32 = 0x2BB5A6
        public static let accentHover: UInt32 = 0x5FD0C3
        public static let accentInk: UInt32 = 0x04201D
        public static let accentSoftText: UInt32 = 0x8FDCD2
        public static let stateAvailable: UInt32 = 0x4CC38A
        public static let stateAvailableText: UInt32 = 0x6FD6A3
        public static let stateMissing: UInt32 = 0xF07A5A
        public static let stateMissingText: UInt32 = 0xF59C84
        public static let stateUpcoming: UInt32 = 0x6EA8FE
        public static let stateUpcomingText: UInt32 = 0x8DBBFF
        public static let stateAttention: UInt32 = 0xF2B544
        public static let stateAttentionText: UInt32 = 0xF5D08A
        public static let stateNeutral: UInt32 = 0xB5BAC3
        public static let stateNeutralBg: UInt32 = 0x22262E
        public static let brandCabinet: UInt32 = 0x0F7A71
        public static let brandPlay: UInt32 = 0x2BB5A6
        public static let brandCream: UInt32 = 0xF2EEE6
    }

    public enum Radius {
        public static let control: Double = 10
        public static let card: Double = 14
        public static let cardTv: Double = 20
        public static let pill: Double = 999
    }

    public enum Space {
        public static let space4: Double = 4
        public static let space8: Double = 8
        public static let space12: Double = 12
        public static let space16: Double = 16
        public static let space24: Double = 24
        public static let space32: Double = 32
        public static let space48: Double = 48
    }

    public enum PhoneType {
        public static let hero: Double = 34
        public static let page: Double = 28
        public static let section: Double = 20
        public static let body: Double = 15
        public static let meta: Double = 13
        public static let small: Double = 12
    }

    public enum TVType {
        public static let hero: Double = 76
        public static let page: Double = 57
        public static let section: Double = 38
        public static let body: Double = 29
        public static let meta: Double = 25
        public static let small: Double = 23
    }

    public enum DurationMs {
        public static let focus: Double = 150
    }

    public enum TVLayout {
        public static let safeSide: Double = 96
        public static let safeTopBottom: Double = 60
        public static let posterWidth: Double = 240
        public static let posterHeight: Double = 360
        public static let cardWidth: Double = 400
        public static let cardHeight: Double = 225
        public static let focusRing: Double = 4
        public static let focusScale: Double = 1.08
    }
}
