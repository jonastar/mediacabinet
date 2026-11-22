plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// The app's version, shared with the TV app: scripts/set-version.sh android.
val cabinetVersion = providers.gradleProperty("cabinetVersion").get()

/**
 * Play needs a bigger versionCode for every upload, and the phone and TV apps
 * share an ID, so the last digit is theirs: 2026.3.1 is 202603010 on phones and
 * 202603011 on TVs. Patches on an old release stay below the next release.
 */
fun playVersionCode(version: String, formFactor: Int): Int {
    val (year, minor, patch) = version.substringBefore('-').split('.').map(String::toInt)
    return year * 100_000 + minor * 1_000 + patch * 10 + formFactor
}

android {
    namespace = "tv.mediacabinet.phone"
    compileSdk = 37

    defaultConfig {
        // The same ID as the TV app: Google Play ships each on its own form factor track.
        applicationId = "tv.mediacabinet"
        minSdk = 26
        targetSdk = 37
        versionCode = playVersionCode(cabinetVersion, formFactor = 0)
        versionName = cabinetVersion
    }

    // Release signing, from the environment (players/android/README.md). Without
    // CABINET_KEYSTORE a release build is unsigned.
    val keystore = providers.environmentVariable("CABINET_KEYSTORE").orNull
    if (keystore != null) {
        signingConfigs {
            create("release") {
                val password = providers.environmentVariable("CABINET_KEYSTORE_PASSWORD").get()
                storeFile = file(keystore)
                storePassword = password
                keyAlias = providers.environmentVariable("CABINET_KEY_ALIAS").get()
                keyPassword =
                    providers.environmentVariable("CABINET_KEY_PASSWORD").orElse(password).get()
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release")
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

dependencies {
    implementation(project(":core"))
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.media3.ui.compose)
    implementation(libs.coil.compose)
    implementation(libs.coil.network.okhttp)
}
