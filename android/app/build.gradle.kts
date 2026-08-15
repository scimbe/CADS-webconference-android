plugins {
    id("com.android.application") version "8.5.0"
    id("org.jetbrains.kotlin.android") version "1.9.24"
}

android {
    namespace = "org.bunsenbrenner.cads.webconference"
    compileSdk = 34

    defaultConfig {
        applicationId = "org.bunsenbrenner.cads.webconference"
        minSdk = 26 // Android Keystore hardware-backing is reliable from here on (gap 3, see ARCHITECTURE.md)
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
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.security:security-crypto:1.1.0-alpha06") // EncryptedSharedPreferences, gap 3
    // Real build error, live-caught: org.webrtc:google-webrtc was only ever distributed via
    // JCenter, shut down years ago -- unresolvable from Google's/Maven Central's real repos.
    // com.infobip:google-webrtc is the actively maintained Maven Central replacement.
    implementation("com.infobip:google-webrtc:1.0.45036")
    implementation(project(":rust-core"))

    // Instrumented tests (KeyStoreIdentityTest) -- EncryptedSharedPreferences/Keystore need
    // a real Android runtime, not a JVM unit test, hence androidTest not test.
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
}
