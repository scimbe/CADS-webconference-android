plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "org.bunsenbrenner.webconference"
    compileSdk = 34

    defaultConfig {
        applicationId = "org.bunsenbrenner.webconference"
        // Minimum SDK for a real RTCPeerConnection (WebRTC) client -- matches
        // ct-agent-wasm's browser baseline, not picked arbitrarily.
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0-scaffold"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    testOptions {
        unitTests {
            isIncludeAndroidResources = true
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")

    // JVM unit tests via Robolectric -- makes the pipeline's "test" stage real for
    // this project (assembleDebug only proves the scaffold compiles, not that it
    // behaves correctly). No emulator/device needed.
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.13")
    testImplementation("androidx.test:core:1.6.1")
}
