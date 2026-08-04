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

    // Required by the UniFFI-generated Kotlin bindings in
    // app/src/main/java/uniffi/native_bridge/native_bridge.kt (com.sun.jna.* imports)
    // -- those bindings make JNA-based FFI calls into libnative_bridge.so. The
    // `@aar` classifier is required on Android per UniFFI's own Kotlin Gradle guide
    // (https://mozilla.github.io/uniffi-rs/latest/kotlin/gradle.html, which asks for
    // 5.12.0+); 5.17.0 is the current stable release.
    //
    // kotlinx-coroutines-core is deliberately NOT added: verified by reading the
    // generated bindings that this spike's single function (bridge_version, not
    // async) produces zero coroutines references -- the file's "// Async support"
    // section is an empty comment header. Add it if/when a `#[uniffi::export(async)]`
    // function is introduced.
    implementation("net.java.dev.jna:jna:5.17.0@aar")

    // JVM unit tests via Robolectric -- makes the pipeline's "test" stage real for
    // this project (assembleDebug only proves the scaffold compiles, not that it
    // behaves correctly). No emulator/device needed.
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.16.1")
    testImplementation("androidx.test:core:1.6.1")
}
