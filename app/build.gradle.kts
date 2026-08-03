import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// No `org.jetbrains.kotlin.android` plugin: AGP 9's built-in Kotlin support supplies
// it automatically (see the root build.gradle.kts comment).
plugins {
    id("com.android.application")
}

android {
    namespace = "org.bunsenbrenner.webconference"
    // 35, not 34: androidx.core 1.16.0 (a transitive dependency of Material 1.14.0)
    // hard-requires compileSdk >= 35 (AAR metadata check, verified against the real
    // failure in Dependabot PR #5's run 30861733892).
    compileSdk = 35

    defaultConfig {
        applicationId = "org.bunsenbrenner.webconference"
        // Minimum SDK for a real RTCPeerConnection (WebRTC) client -- matches
        // ct-agent-wasm's browser baseline, not picked arbitrarily.
        minSdk = 26
        targetSdk = 35
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
    testOptions {
        unitTests {
            isIncludeAndroidResources = true
        }
    }
}

// jvmTarget configuration lives in the top-level `kotlin {}` extension (the modern
// compilerOptions DSL, not the deprecated `kotlinOptions` inside android{}) -- still
// available and functional under AGP's built-in Kotlin support.
kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.14.0")

    // JVM unit tests via Robolectric -- makes the pipeline's "test" stage real for
    // this project (assembleDebug only proves the scaffold compiles, not that it
    // behaves correctly). No emulator/device needed.
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.16.1")
    testImplementation("androidx.test:core:1.6.1")
}
