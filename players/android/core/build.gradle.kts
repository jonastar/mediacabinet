plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "app.mediacabinet.core"
    compileSdk = 37

    defaultConfig {
        minSdk = 26
    }

    // OkHttp looks for Android's logger, which isn't there in JVM tests.
    testOptions.unitTests.isReturnDefaultValues = true
}

dependencies {
    api(libs.okhttp)
    api(libs.serialization.json)
    api(libs.coroutines.android)
    api(libs.media3.exoplayer)
    implementation(libs.media3.exoplayer.hls)
    implementation(libs.media3.datasource.okhttp)

    testImplementation(libs.junit)
    testImplementation(libs.coroutines.test)
    testImplementation(libs.okhttp.mockwebserver)
}
